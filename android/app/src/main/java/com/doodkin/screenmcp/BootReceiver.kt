package com.doodkin.screenmcp

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import com.google.firebase.auth.FirebaseAuth
import java.security.SecureRandom

/**
 * Starts the ScreenMCP connection service on device boot.
 * Only activates if "start_on_boot" is enabled in SharedPreferences.
 * Works on the lock screen — no UI needed.
 */
class BootReceiver : BroadcastReceiver() {

    companion object {
        private const val TAG = "BootReceiver"
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return

        val prefs = context.getSharedPreferences("screenmcp", Context.MODE_PRIVATE)
        val startOnBoot = prefs.getBoolean("start_on_boot", false)
        if (!startOnBoot) {
            Log.d(TAG, "start_on_boot disabled, skipping")
            return
        }

        Log.i(TAG, "Boot completed — starting ScreenMCP service")

        val ossEnabled = prefs.getBoolean("opensource_server_enabled", false)
        val useSse = prefs.getBoolean("use_sse", false)

        if (ossEnabled) {
            // Open source mode — start SSE service
            val userId = prefs.getString("opensource_user_id", "") ?: ""
            val apiUrl = prefs.getString("opensource_api_url", "") ?: ""
            if (userId.isEmpty() || apiUrl.isEmpty()) {
                Log.w(TAG, "OSS mode but missing user_id or api_url")
                return
            }
            val sseIntent = Intent(context, SseService::class.java).apply {
                putExtra("user_id", userId)
                putExtra("api_url", apiUrl)
            }
            context.startForegroundService(sseIntent)
        } else if (useSse) {
            // Firebase + SSE mode — need Firebase token
            startWithFirebaseToken(context, useSse = true)
        } else {
            // Firebase + FCM mode — ConnectionService will be started by FCM push
            // But we can also proactively connect via discover
            startWithFirebaseToken(context, useSse = false)
        }
    }

    private fun startWithFirebaseToken(context: Context, useSse: Boolean) {
        val user = FirebaseAuth.getInstance().currentUser
        if (user == null) {
            Log.w(TAG, "No Firebase user signed in, cannot auto-start")
            return
        }

        user.getIdToken(false).addOnSuccessListener { result ->
            val token = result.token ?: return@addOnSuccessListener
            val apiUrl = "https://api.screenmcp.com"
            val deviceId = getDeviceId(context)

            if (useSse) {
                val intent = Intent(context, SseService::class.java).apply {
                    putExtra("user_id", token)
                    putExtra("api_url", apiUrl)
                    putExtra("firebase_mode", true)
                }
                context.startForegroundService(intent)
            } else {
                // Discover worker URL and connect
                Thread {
                    try {
                        val discoverUrl = "$apiUrl/api/discover"
                        val conn = java.net.URL(discoverUrl).openConnection() as java.net.HttpURLConnection
                        conn.requestMethod = "POST"
                        conn.setRequestProperty("Authorization", "Bearer $token")
                        conn.setRequestProperty("Content-Type", "application/json")
                        conn.doOutput = true
                        conn.outputStream.write("{}".toByteArray())

                        if (conn.responseCode == 200) {
                            val body = conn.inputStream.bufferedReader().readText()
                            val json = org.json.JSONObject(body)
                            val wsUrl = json.optString("wsUrl", "")
                            if (wsUrl.isNotEmpty()) {
                                val intent = Intent(context, ConnectionService::class.java).apply {
                                    putExtra(ConnectionService.EXTRA_WS_URL, wsUrl)
                                    putExtra(ConnectionService.EXTRA_API_URL, apiUrl)
                                    putExtra(ConnectionService.EXTRA_TOKEN, token)
                                    putExtra(ConnectionService.EXTRA_DEVICE_ID, deviceId)
                                }
                                context.startForegroundService(intent)
                                Log.i(TAG, "Started ConnectionService → $wsUrl")
                            }
                        }
                        conn.disconnect()
                    } catch (e: Exception) {
                        Log.e(TAG, "Failed to discover and connect: ${e.message}")
                    }
                }.start()
            }
        }.addOnFailureListener {
            Log.e(TAG, "Failed to get Firebase token: ${it.message}")
        }
    }

    private fun getDeviceId(context: Context): String {
        val prefs = context.getSharedPreferences("screenmcp", Context.MODE_PRIVATE)
        var deviceId = prefs.getString("device_id", null)
        if (deviceId.isNullOrEmpty()) {
            val bytes = ByteArray(16)
            SecureRandom().nextBytes(bytes)
            deviceId = bytes.joinToString("") { "%02x".format(it) }
            prefs.edit().putString("device_id", deviceId).apply()
        }
        return deviceId
    }
}
