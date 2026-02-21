package com.doodkin.screenmcp

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import androidx.core.app.NotificationCompat
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.sse.EventSource
import okhttp3.sse.EventSourceListener
import okhttp3.sse.EventSources
import org.json.JSONObject
import java.security.SecureRandom
import java.util.concurrent.TimeUnit

/**
 * Foreground service that listens for Server-Sent Events (SSE) from an open source server.
 * Replaces FCM push notifications in open source mode.
 *
 * Listens on {apiUrl}/api/events with Bearer token = userId.
 * Handles "connect" events to initiate WebSocket connections to workers.
 */
class SseService : Service() {

    companion object {
        private const val TAG = "SseService"
        private const val CHANNEL_ID = "screenmcp_sse"
        private const val NOTIFICATION_ID = 2
        private const val MAX_RECONNECT_DELAY_MS = 60_000L
        private const val INITIAL_RECONNECT_DELAY_MS = 1_000L

        var instance: SseService? = null
            private set

        fun start(context: Context) {
            val prefs = context.getSharedPreferences("screenmcp", Context.MODE_PRIVATE)
            val enabled = prefs.getBoolean("opensource_server_enabled", false)
            if (!enabled) return

            val userId = prefs.getString("opensource_user_id", "") ?: ""
            val apiUrl = prefs.getString("opensource_api_url", "") ?: ""
            if (userId.isEmpty() || apiUrl.isEmpty()) return

            val intent = Intent(context, SseService::class.java).apply {
                putExtra("user_id", userId)
                putExtra("api_url", apiUrl)
            }
            context.startForegroundService(intent)
        }

        fun stop(context: Context) {
            val intent = Intent(context, SseService::class.java)
            context.stopService(intent)
        }
    }

    private var eventSource: EventSource? = null
    private var userId: String? = null
    private var apiUrl: String? = null
    private val handler = Handler(Looper.getMainLooper())
    private var reconnectDelay = INITIAL_RECONNECT_DELAY_MS
    private var shouldReconnect = true

    private val httpClient = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS) // no read timeout for SSE
        .connectTimeout(30, TimeUnit.SECONDS)
        .build()

    override fun onCreate() {
        super.onCreate()
        instance = this
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val newUserId = intent?.getStringExtra("user_id")
        val newApiUrl = intent?.getStringExtra("api_url")

        startForeground(NOTIFICATION_ID, buildNotification("Connecting to SSE..."))

        if (newUserId != null && newApiUrl != null) {
            // Disconnect existing SSE if parameters changed
            if (newUserId != userId || newApiUrl != apiUrl) {
                disconnectSse()
            }
            userId = newUserId
            apiUrl = newApiUrl
            shouldReconnect = true
            reconnectDelay = INITIAL_RECONNECT_DELAY_MS
            connectSse()
        }

        return START_STICKY
    }

    override fun onDestroy() {
        shouldReconnect = false
        disconnectSse()
        handler.removeCallbacksAndMessages(null)
        instance = null
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun connectSse() {
        val url = apiUrl ?: return
        val token = userId ?: return

        Log.i(TAG, "Connecting SSE to $url/api/events")
        updateNotification("Connecting to SSE...")

        val request = Request.Builder()
            .url("$url/api/events")
            .addHeader("Authorization", "Bearer $token")
            .addHeader("Accept", "text/event-stream")
            .build()

        val factory = EventSources.createFactory(httpClient)
        eventSource = factory.newEventSource(request, object : EventSourceListener() {
            override fun onOpen(eventSource: EventSource, response: Response) {
                Log.i(TAG, "SSE connected")
                reconnectDelay = INITIAL_RECONNECT_DELAY_MS
                handler.post { updateNotification("SSE connected") }
            }

            override fun onEvent(eventSource: EventSource, id: String?, type: String?, data: String) {
                Log.i(TAG, "SSE event: type=$type data=$data")
                handleSseEvent(data)
            }

            override fun onClosed(eventSource: EventSource) {
                Log.i(TAG, "SSE closed")
                handler.post { updateNotification("SSE disconnected") }
                scheduleReconnect()
            }

            override fun onFailure(eventSource: EventSource, t: Throwable?, response: Response?) {
                Log.e(TAG, "SSE failure: ${t?.message}, response=${response?.code}")
                handler.post { updateNotification("SSE connection failed") }
                scheduleReconnect()
            }
        })
    }

    private fun disconnectSse() {
        eventSource?.cancel()
        eventSource = null
    }

    private fun handleSseEvent(data: String) {
        try {
            val json = JSONObject(data)
            val type = json.optString("type", "")

            when (type) {
                "connect" -> handleConnectEvent(json)
                else -> Log.d(TAG, "Ignoring SSE event type: $type")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to parse SSE event: ${e.message}")
        }
    }

    private fun handleConnectEvent(json: JSONObject) {
        val wsUrl = json.optString("wsUrl", "")
        val targetDeviceId = json.optString("target_device_id", "")

        if (wsUrl.isEmpty()) {
            Log.w(TAG, "SSE connect event missing wsUrl")
            return
        }

        val myDeviceId = getDeviceUUID()

        // If target_device_id is specified and doesn't match this device, ignore
        if (targetDeviceId.isNotEmpty() && targetDeviceId != myDeviceId) {
            Log.i(TAG, "SSE connect not for this device (target=$targetDeviceId, mine=$myDeviceId), ignoring")
            return
        }

        Log.i(TAG, "SSE connect request: wsUrl=$wsUrl")

        val token = userId ?: return
        val api = apiUrl ?: return

        // Skip if already connected to the same worker
        val mcpService = ScreenMcpService.instance
        if (mcpService != null && mcpService.isWorkerConnectedTo(wsUrl)) {
            Log.i(TAG, "Already connected to $wsUrl, ignoring SSE connect")
            return
        }

        // Start ConnectionService for foreground notification + connection
        handler.post {
            val intent = Intent(this, ConnectionService::class.java).apply {
                putExtra(ConnectionService.EXTRA_WS_URL, wsUrl)
                putExtra(ConnectionService.EXTRA_API_URL, api)
                putExtra(ConnectionService.EXTRA_TOKEN, token)
                putExtra(ConnectionService.EXTRA_DEVICE_ID, myDeviceId)
            }
            startForegroundService(intent)
        }
    }

    private fun getDeviceUUID(): String {
        val prefs = getSharedPreferences("screenmcp", MODE_PRIVATE)
        var deviceId = prefs.getString("device_id", null)
        if (deviceId.isNullOrEmpty()) {
            val bytes = ByteArray(16)
            SecureRandom().nextBytes(bytes)
            deviceId = bytes.joinToString("") { "%02x".format(it) }
            prefs.edit().putString("device_id", deviceId).apply()
        }
        return deviceId
    }

    private fun scheduleReconnect() {
        if (!shouldReconnect) return

        Log.i(TAG, "Scheduling SSE reconnect in ${reconnectDelay}ms")
        handler.postDelayed({
            if (shouldReconnect) {
                connectSse()
            }
        }, reconnectDelay)

        // Exponential backoff
        reconnectDelay = minOf(reconnectDelay * 2, MAX_RECONNECT_DELAY_MS)
    }

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "ScreenMCP SSE",
            NotificationManager.IMPORTANCE_LOW
        ).apply {
            description = "Shows ScreenMCP SSE connection status"
        }
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(channel)
    }

    private fun buildNotification(status: String): Notification {
        val pendingIntent = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("ScreenMCP SSE")
            .setContentText(status)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .build()
    }

    private fun updateNotification(status: String) {
        val manager = getSystemService(NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, buildNotification(status))
    }
}
