use redis::AsyncCommands;
use tracing::{info, warn};

use crate::protocol::Command;

/// Redis state manager for session resumption and command tracking.
///
/// Keys:
///   user:{uid}:server       → worker domain holding the connection
///   user:{uid}:pending      → JSON list of unacked commands
///   user:{uid}:last_ack     → last command ID the phone confirmed
///   user:{uid}:cmd_counter  → next command ID to assign
pub struct State {
    redis: redis::Client,
    worker_id: String,
}

impl State {
    pub fn new(redis_url: &str, worker_id: String) -> Result<Self, redis::RedisError> {
        let redis = redis::Client::open(redis_url)?;
        Ok(Self { redis, worker_id })
    }

    async fn conn(&self) -> Result<redis::aio::MultiplexedConnection, redis::RedisError> {
        self.redis.get_multiplexed_async_connection().await
    }

    /// Register this connection: set the worker holding this user's session.
    pub async fn register_connection(&self, uid: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.conn().await?;
        let key = format!("user:{uid}:server");
        conn.set::<_, _, ()>(&key, &self.worker_id).await?;
        info!(uid, worker = %self.worker_id, "registered connection");
        Ok(())
    }

    /// Clean up when a phone disconnects.
    pub async fn unregister_connection(&self, uid: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.conn().await?;
        let key = format!("user:{uid}:server");
        // Only delete if we still own it
        let current: Option<String> = conn.get(&key).await?;
        if current.as_deref() == Some(&self.worker_id) {
            conn.del::<_, ()>(&key).await?;
            info!(uid, "unregistered connection");
        }
        Ok(())
    }

    /// Get the last ack value stored in Redis.
    pub async fn get_last_ack(&self, uid: &str) -> Result<i64, redis::RedisError> {
        let mut conn = self.conn().await?;
        let key = format!("user:{uid}:last_ack");
        let val: Option<i64> = conn.get(&key).await?;
        Ok(val.unwrap_or(0))
    }

    /// Update last_ack and remove acked commands from pending.
    pub async fn process_ack(&self, uid: &str, ack_id: i64) -> Result<(), redis::RedisError> {
        let mut conn = self.conn().await?;

        // Update last_ack
        let ack_key = format!("user:{uid}:last_ack");
        conn.set::<_, _, ()>(&ack_key, ack_id).await?;

        // Remove acked commands from pending list
        let pending_key = format!("user:{uid}:pending");
        let pending: Option<String> = conn.get(&pending_key).await?;

        if let Some(pending_json) = pending {
            if let Ok(commands) = serde_json::from_str::<Vec<Command>>(&pending_json) {
                let remaining: Vec<&Command> = commands.iter().filter(|c| c.id > ack_id).collect();
                let new_json = serde_json::to_string(&remaining).unwrap_or_else(|_| "[]".into());
                conn.set::<_, _, ()>(&pending_key, &new_json).await?;
            }
        }

        info!(uid, ack_id, "processed ack");
        Ok(())
    }

    /// Get pending commands that need to be replayed (id > last_ack).
    pub async fn get_pending_commands(
        &self,
        uid: &str,
        since_ack: i64,
    ) -> Result<Vec<Command>, redis::RedisError> {
        let mut conn = self.conn().await?;
        let pending_key = format!("user:{uid}:pending");
        let pending: Option<String> = conn.get(&pending_key).await?;

        let commands = match pending {
            Some(json) => serde_json::from_str::<Vec<Command>>(&json).unwrap_or_default(),
            None => vec![],
        };

        let replay: Vec<Command> = commands.into_iter().filter(|c| c.id > since_ack).collect();
        info!(uid, since_ack, count = replay.len(), "pending commands to replay");
        Ok(replay)
    }

    /// Enqueue a new command: assign an ID, append to pending, return the command.
    pub async fn enqueue_command(
        &self,
        uid: &str,
        cmd: String,
        params: Option<serde_json::Value>,
    ) -> Result<Command, redis::RedisError> {
        let mut conn = self.conn().await?;

        // Increment command counter
        let counter_key = format!("user:{uid}:cmd_counter");
        let id: i64 = conn.incr(&counter_key, 1).await?;

        let command = Command {
            id,
            cmd,
            params,
        };

        // Append to pending list
        let pending_key = format!("user:{uid}:pending");
        let pending: Option<String> = conn.get(&pending_key).await?;
        let mut commands: Vec<Command> = match pending {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => vec![],
        };

        // Enforce max pending limit
        if commands.len() >= 50 {
            warn!(uid, "max pending commands reached (50)");
            // Still allow but log warning
        }

        commands.push(command.clone());
        let new_json = serde_json::to_string(&commands).unwrap_or_else(|_| "[]".into());
        conn.set::<_, _, ()>(&pending_key, &new_json).await?;

        info!(uid, id, "enqueued command");
        Ok(command)
    }

    /// Store a command response (for MCP clients to retrieve).
    pub async fn store_response(
        &self,
        uid: &str,
        cmd_id: i64,
        response_json: &str,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.conn().await?;
        let key = format!("user:{uid}:response:{cmd_id}");
        // Store with 5 minute TTL
        conn.set_ex::<_, _, ()>(&key, response_json, 300).await?;
        Ok(())
    }
}
