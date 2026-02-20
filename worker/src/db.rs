use tokio_postgres::NoTls;
use tracing::{error, info};

pub struct Db {
    client: tokio_postgres::Client,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, tokio_postgres::Error> {
        let (client, connection) = tokio_postgres::connect(database_url, NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("postgres connection error: {e}");
            }
        });

        info!("connected to postgres");
        Ok(Self { client })
    }

    /// Look up a user by Firebase UID. Returns (user_id, email).
    pub async fn get_user_by_firebase_uid(
        &self,
        firebase_uid: &str,
    ) -> Result<Option<(String, String)>, tokio_postgres::Error> {
        let row = self
            .client
            .query_opt(
                "SELECT id, email FROM users WHERE firebase_uid = $1",
                &[&firebase_uid],
            )
            .await?;

        Ok(row.map(|r| {
            let id: uuid::Uuid = r.get(0);
            let email: String = r.get(1);
            (id.to_string(), email)
        }))
    }

    /// Upsert a user record. Returns user UUID.
    pub async fn upsert_user(
        &self,
        firebase_uid: &str,
        email: &str,
        name: Option<&str>,
    ) -> Result<String, tokio_postgres::Error> {
        let row = self
            .client
            .query_one(
                "INSERT INTO users (firebase_uid, email, name)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (firebase_uid) DO UPDATE SET email = $2, name = $3
                 RETURNING id",
                &[&firebase_uid, &email, &name],
            )
            .await?;

        let id: uuid::Uuid = row.get(0);
        Ok(id.to_string())
    }

    /// Mark a device as disconnected by device UUID string.
    pub async fn disconnect_device(&self, device_id: &str) -> Result<(), tokio_postgres::Error> {
        if let Ok(uuid) = device_id.parse::<uuid::Uuid>() {
            self.client
                .execute(
                    "UPDATE devices SET connected = false, last_seen = now() WHERE id = $1",
                    &[&uuid],
                )
                .await?;
        }
        Ok(())
    }
}
