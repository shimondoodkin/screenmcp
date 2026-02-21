package com.doodkin.screenmcp

import android.content.Intent
import android.util.Log
import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import java.io.OutputStreamWriter
import java.net.HttpURLConnection
import java.net.URL
import java.security.SecureRandom

class FcmService : FirebaseMessagingService() {

    companion object {
        private const val TAG = "FcmService"
        // API base URL for device registration
        var apiBaseUrl: String = "https://server10.doodkin.com"
    }

    /** Get or create a persistent cryptographically secure device ID in SharedPreferences */
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

    override fun onMessageReceived(message: RemoteMessage) {
        Log.i(TAG, "FCM message received: ${message.data}")

        val type = message.data["type"]
        val wsUrl = message.data["wsUrl"]

        if (type == "connect" && wsUrl != null) {
            Log.i(TAG, "FCM connect request: $wsUrl")

            // Get Firebase token for auth
            val user = FirebaseAuth.getInstance().currentUser
            if (user == null) {
                Log.w(TAG, "No signed-in user, ignoring FCM connect")
                return
            }

            user.getIdToken(false).addOnSuccessListener { result ->
                val idToken = result.token ?: return@addOnSuccessListener

                // Skip if already connected to the same worker
                val mcpService = ScreenMcpService.instance
                if (mcpService != null && mcpService.isWorkerConnectedTo(wsUrl)) {
                    Log.i(TAG, "Already connected to $wsUrl, ignoring FCM")
                    return@addOnSuccessListener
                }

                // Start ConnectionService for foreground notification + connection
                val intent = Intent(this, ConnectionService::class.java).apply {
                    putExtra(ConnectionService.EXTRA_WS_URL, wsUrl)
                    putExtra(ConnectionService.EXTRA_API_URL, apiBaseUrl)
                    putExtra(ConnectionService.EXTRA_TOKEN, idToken)
                    putExtra(ConnectionService.EXTRA_DEVICE_ID, getDeviceUUID())
                }
                startForegroundService(intent)
            }
        }
    }

    override fun onNewToken(token: String) {
        Log.i(TAG, "New FCM token: ${token.take(20)}...")
        registerFcmToken(token)
    }

    private fun registerFcmToken(fcmToken: String) {
        val user = FirebaseAuth.getInstance().currentUser ?: return

        user.getIdToken(false).addOnSuccessListener { result ->
            val idToken = result.token ?: return@addOnSuccessListener

            Thread {
                try {
                    val url = URL("$apiBaseUrl/api/devices/register")
                    val conn = url.openConnection() as HttpURLConnection
                    conn.requestMethod = "POST"
                    conn.setRequestProperty("Content-Type", "application/json")
                    conn.setRequestProperty("Authorization", "Bearer $idToken")
                    conn.doOutput = true

                    val body = org.json.JSONObject().apply {
                        put("fcmToken", fcmToken)
                        put("deviceName", android.os.Build.MODEL)
                        put("deviceModel", "${android.os.Build.MANUFACTURER} ${android.os.Build.MODEL}")
                        put("deviceId", getDeviceUUID())
                    }

                    OutputStreamWriter(conn.outputStream).use {
                        it.write(body.toString())
                    }

                    val responseCode = conn.responseCode
                    Log.i(TAG, "FCM token registered: $responseCode")
                    conn.disconnect()
                } catch (e: Exception) {
                    Log.e(TAG, "Failed to register FCM token: ${e.message}")
                }
            }.start()
        }
    }
}
