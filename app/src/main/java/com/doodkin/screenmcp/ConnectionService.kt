package com.doodkin.screenmcp

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat

/**
 * Thin foreground service for the persistent notification.
 * The actual WebSocket connection lives in ScreenMcpService (AccessibilityService).
 */
class ConnectionService : Service() {

    companion object {
        private const val TAG = "ConnectionService"
        private const val CHANNEL_ID = "screenmcp_connection"
        private const val NOTIFICATION_ID = 1
        const val EXTRA_WS_URL = "ws_url"
        const val EXTRA_API_URL = "api_url"
        const val EXTRA_TOKEN = "token"

        var instance: ConnectionService? = null
            private set
    }

    private var currentStatus = "Disconnected"

    override fun onCreate() {
        super.onCreate()
        instance = this
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val token = intent?.getStringExtra(EXTRA_TOKEN)
        val wsUrl = intent?.getStringExtra(EXTRA_WS_URL)
        val apiUrl = intent?.getStringExtra(EXTRA_API_URL)

        startForeground(NOTIFICATION_ID, buildNotification("Connecting..."))

        val service = ScreenMcpService.instance
        if (service == null) {
            Log.w(TAG, "ScreenMcpService not available, accessibility not enabled?")
            updateNotification("Waiting for accessibility service...")
            return START_STICKY
        }

        // Register for status updates from ScreenMcpService
        service.onConnectionStatusChange = { status ->
            currentStatus = status
            Log.i(TAG, "Status: $status")
            updateNotification(status)
        }

        if (token != null) {
            // Skip if already connected to the same worker
            if (wsUrl != null && service.isWorkerConnectedTo(wsUrl)) {
                Log.i(TAG, "Already connected to $wsUrl, skipping")
                return START_STICKY
            }

            service.disconnectWorker()

            if (wsUrl != null) {
                Log.i(TAG, "Direct connect to $wsUrl (fallback API: $apiUrl)")
                service.connectDirect(wsUrl, token, fallbackApiUrl = apiUrl)
            } else if (apiUrl != null) {
                Log.i(TAG, "Discover via $apiUrl")
                service.connectViaApi(apiUrl, token)
            }
        }

        return START_STICKY
    }

    override fun onDestroy() {
        ScreenMcpService.instance?.onConnectionStatusChange = null
        instance = null
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    fun disconnect() {
        ScreenMcpService.instance?.disconnectWorker()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    fun isConnected(): Boolean = ScreenMcpService.instance?.isWorkerConnected() == true
    fun getStatus(): String = currentStatus

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "ScreenMCP Connection",
            NotificationManager.IMPORTANCE_LOW
        ).apply {
            description = "Shows ScreenMCP connection status"
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
            .setContentTitle("ScreenMCP")
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
