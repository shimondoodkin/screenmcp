package com.doodkin.screenmcp

import android.accessibilityservice.AccessibilityService
import android.graphics.Bitmap
import android.os.Handler
import android.os.Looper
import android.util.Base64
import android.util.Log
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class WebSocketClient(
    private val onStatusChange: (String) -> Unit
) {
    companion object {
        private const val TAG = "WebSocketClient"
        private const val MAX_RECONNECT_DELAY_MS = 30_000L
        private const val MAX_RECONNECT_ATTEMPTS = 5
        private const val IDLE_TIMEOUT_MS = 5 * 60_000L // 5 minutes
    }

    private var webSocket: WebSocket? = null
    private var lastWorkerUrl: String? = null
    private var apiUrl: String? = null
    private var token: String? = null
    private var deviceId: String? = null
    private val httpClient = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(30, TimeUnit.SECONDS)
        .build()

    private val isConnected = AtomicBoolean(false)
    private val isConnecting = AtomicBoolean(false)
    private val shouldReconnect = AtomicBoolean(false)
    private var reconnectAttempt = 0
    private val handler = Handler(Looper.getMainLooper())
    private var idleRunnable: Runnable? = null
    /** Monotonic generation counter — stale callbacks compare against this to bail out */
    @Volatile private var connectionGeneration = 0L

    /**
     * Connect via discovery API: call /api/discover to get a worker URL, then WS connect.
     */
    fun connectViaApi(apiUrl: String, token: String, deviceId: String? = null) {
        // Tear down any existing connection first
        closeQuietly()
        this.apiUrl = apiUrl
        this.token = token
        this.deviceId = deviceId
        shouldReconnect.set(true)
        reconnectAttempt = 0
        connectionGeneration++
        discoverAndConnect()
    }

    /**
     * Connect directly to a known worker URL (for manual/FCM-provided URLs).
     * If fallbackApiUrl is provided, reconnection will use API discovery instead of retrying the same URL.
     */
    fun connectDirect(workerUrl: String, token: String, fallbackApiUrl: String? = null, deviceId: String? = null) {
        // Tear down any existing connection first
        closeQuietly()
        this.apiUrl = fallbackApiUrl
        this.token = token
        this.deviceId = deviceId
        this.lastWorkerUrl = workerUrl
        shouldReconnect.set(true)
        reconnectAttempt = 0
        connectionGeneration++
        doConnect(workerUrl)
    }

    fun disconnect() {
        shouldReconnect.set(false)
        connectionGeneration++
        handler.removeCallbacksAndMessages(null)
        closeQuietly()
        onStatusChange("Disconnected")
    }

    /** Close the current websocket without triggering reconnect */
    private fun closeQuietly() {
        isConnecting.set(false)
        isConnected.set(false)
        val ws = webSocket
        webSocket = null
        ws?.close(1000, "replaced")
    }

    fun isConnected(): Boolean = isConnected.get()

    fun isConnectedTo(url: String): Boolean = isConnected.get() && lastWorkerUrl == url

    private fun discoverAndConnect() {
        val api = apiUrl ?: return
        val tok = token ?: return
        val myGeneration = connectionGeneration

        onStatusChange("Discovering worker...")
        Log.i(TAG, "Calling discovery API: $api/api/discover (gen=$myGeneration)")

        Thread {
            try {
                if (myGeneration != connectionGeneration) return@Thread

                val body = "{}".toRequestBody("application/json".toMediaType())
                val request = Request.Builder()
                    .url("$api/api/discover")
                    .post(body)
                    .addHeader("Authorization", "Bearer $tok")
                    .build()

                val response = httpClient.newCall(request).execute()
                val responseBody = response.body?.string() ?: "{}"

                if (myGeneration != connectionGeneration) return@Thread

                if (!response.isSuccessful) {
                    Log.e(TAG, "Discovery failed: ${response.code} $responseBody")
                    handler.post { onStatusChange("Discovery failed") }
                    scheduleReconnect()
                    return@Thread
                }

                val json = JSONObject(responseBody)
                val wsUrl = json.optString("wsUrl", "")
                if (wsUrl.isEmpty()) {
                    Log.e(TAG, "Discovery returned no wsUrl")
                    handler.post { onStatusChange("No worker available") }
                    scheduleReconnect()
                    return@Thread
                }

                Log.i(TAG, "Discovered worker: $wsUrl")
                lastWorkerUrl = wsUrl
                handler.post { doConnect(wsUrl) }
            } catch (e: Exception) {
                if (myGeneration != connectionGeneration) return@Thread
                Log.e(TAG, "Discovery error: ${e.message}")
                handler.post { onStatusChange("Discovery error") }
                scheduleReconnect()
            }
        }.start()
    }

    private fun doConnect(wsUrl: String) {
        val wsToken = token ?: return

        // Prevent concurrent connection attempts
        if (!isConnecting.compareAndSet(false, true)) {
            Log.w(TAG, "Already connecting, ignoring duplicate")
            return
        }

        val myGeneration = connectionGeneration

        onStatusChange("Connecting to $wsUrl...")
        Log.i(TAG, "Connecting to $wsUrl (gen=$myGeneration)")

        val request = Request.Builder().url(wsUrl).build()

        webSocket = httpClient.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(ws: WebSocket, response: Response) {
                if (myGeneration != connectionGeneration) { ws.close(1000, "stale"); return }
                Log.i(TAG, "WebSocket opened, sending auth")
                val auth = JSONObject().apply {
                    put("type", "auth")
                    put("user_id", wsToken)
                    put("role", "phone")
                    put("last_ack", 0)
                    deviceId?.let { put("device_id", it) }
                }
                ws.send(auth.toString())
            }

            override fun onMessage(ws: WebSocket, text: String) {
                if (myGeneration != connectionGeneration) return
                handleMessage(ws, text)
            }

            override fun onClosing(ws: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "WebSocket closing: $code $reason")
                ws.close(code, reason)
            }

            override fun onClosed(ws: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "WebSocket closed: $code $reason")
                isConnected.set(false)
                isConnecting.set(false)
                if (myGeneration != connectionGeneration) return
                handler.post { onStatusChange("Disconnected") }
                scheduleReconnect()
            }

            override fun onFailure(ws: WebSocket, t: Throwable, response: Response?) {
                Log.e(TAG, "WebSocket failure: ${t.message}")
                isConnected.set(false)
                isConnecting.set(false)
                if (myGeneration != connectionGeneration) return
                handler.post { onStatusChange("Connection failed") }
                scheduleReconnect()
            }
        })
    }

    private fun handleMessage(ws: WebSocket, text: String) {
        try {
            val json = JSONObject(text)
            val type = json.optString("type", "")

            when (type) {
                "auth_ok" -> {
                    Log.i(TAG, "Authenticated successfully")
                    isConnected.set(true)
                    isConnecting.set(false)
                    reconnectAttempt = 0
                    handler.post { onStatusChange("Connected") }
                    resetIdleTimer()
                }
                "auth_fail" -> {
                    Log.e(TAG, "Auth failed: ${json.optString("error")}")
                    isConnected.set(false)
                    shouldReconnect.set(false)
                    handler.post { onStatusChange("Auth failed") }
                }
                "ping" -> {
                    ws.send(JSONObject().put("type", "pong").toString())
                    resetIdleTimer()
                }
                "error" -> {
                    Log.e(TAG, "Server error: ${json.optString("error")}")
                }
                else -> {
                    if (json.has("id") && json.has("cmd")) {
                        resetIdleTimer()
                        executeCommand(ws, json)
                    }
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to handle message: ${e.message}")
        }
    }

    private fun executeCommand(ws: WebSocket, json: JSONObject) {
        val id = json.getLong("id")
        val cmd = json.getString("cmd")
        val params = json.optJSONObject("params")

        Log.i(TAG, "Executing command $id: $cmd")

        val service = ScreenMcpService.instance
        if (service == null) {
            sendResponse(ws, id, "error", error = "accessibility service not connected")
            return
        }

        when (cmd) {
            "screenshot" -> {
                if (service.isPhoneLocked()) {
                    sendResponse(ws, id, "error", error = "phone is locked")
                    return
                }
                val quality = params?.optInt("quality", 100) ?: 100
                val maxWidth = if (params?.has("max_width") == true) params.optInt("max_width") else null
                val maxHeight = if (params?.has("max_height") == true) params.optInt("max_height") else null

                service.takeScreenshot(object : AccessibilityService.TakeScreenshotCallback {
                    override fun onSuccess(result: AccessibilityService.ScreenshotResult) {
                        try {
                            val hwBuffer = result.hardwareBuffer
                            val colorSpace = result.colorSpace
                            val bitmap = Bitmap.wrapHardwareBuffer(hwBuffer, colorSpace)
                            if (bitmap == null) {
                                sendResponse(ws, id, "error", error = "failed to create bitmap")
                                return
                            }
                            var softBitmap = bitmap.copy(Bitmap.Config.ARGB_8888, false)
                            bitmap.recycle()
                            hwBuffer.close()

                            softBitmap = service.scaleBitmap(softBitmap, maxWidth, maxHeight)
                            val bytes = service.compressToWebP(softBitmap, quality)
                            softBitmap.recycle()

                            val base64 = Base64.encodeToString(bytes, Base64.NO_WRAP)
                            sendResponse(ws, id, "ok", JSONObject().put("image", base64))
                        } catch (e: Exception) {
                            sendResponse(ws, id, "error", error = e.message)
                        }
                    }

                    override fun onFailure(errorCode: Int) {
                        sendResponse(ws, id, "error", error = "screenshot failed: $errorCode")
                    }
                })
            }

            "ui_tree" -> {
                val tree = service.getUiTreeJson()
                sendResponse(ws, id, "ok", JSONObject().put("tree", tree))
            }

            "click" -> {
                val x = params?.optDouble("x")?.toFloat() ?: run {
                    sendResponse(ws, id, "error", error = "missing x param"); return
                }
                val y = params.optDouble("y").toFloat()
                val duration = params.optLong("duration", 100)
                service.click(x, y, duration, object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "ok")
                    }
                    override fun onCancelled(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "error", error = "gesture cancelled")
                    }
                })
            }

            "long_click" -> {
                val x = params?.optDouble("x")?.toFloat() ?: run {
                    sendResponse(ws, id, "error", error = "missing x param"); return
                }
                val y = params.optDouble("y").toFloat()
                service.longClick(x, y, object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "ok")
                    }
                    override fun onCancelled(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "error", error = "gesture cancelled")
                    }
                })
            }

            "drag" -> {
                val sx = params?.optDouble("startX")?.toFloat() ?: run {
                    sendResponse(ws, id, "error", error = "missing startX param"); return
                }
                val sy = params.optDouble("startY").toFloat()
                val ex = params.optDouble("endX").toFloat()
                val ey = params.optDouble("endY").toFloat()
                val duration = params.optLong("duration", 300)
                service.drag(sx, sy, ex, ey, duration, object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "ok")
                    }
                    override fun onCancelled(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "error", error = "gesture cancelled")
                    }
                })
            }

            "type" -> {
                val text = params?.optString("text", "") ?: ""
                val success = service.typeText(text)
                sendResponse(ws, id, if (success) "ok" else "error",
                    error = if (!success) "no focused input field" else null)
            }

            "get_text" -> {
                val text = service.getTextFromFocused()
                if (text != null) {
                    sendResponse(ws, id, "ok", JSONObject().put("text", text))
                } else {
                    sendResponse(ws, id, "error", error = "no focused input field")
                }
            }

            "back" -> { service.pressBack(); sendResponse(ws, id, "ok") }
            "home" -> { service.pressHome(); sendResponse(ws, id, "ok") }
            "recents" -> { service.pressRecents(); sendResponse(ws, id, "ok") }

            "select_all" -> {
                val ok = service.selectAll()
                sendResponse(ws, id, if (ok) "ok" else "error",
                    error = if (!ok) "select all failed" else null)
            }
            "copy" -> {
                val ok = service.copy()
                sendResponse(ws, id, if (ok) "ok" else "error",
                    error = if (!ok) "copy failed" else null)
            }
            "paste" -> {
                val ok = service.paste()
                sendResponse(ws, id, if (ok) "ok" else "error",
                    error = if (!ok) "paste failed" else null)
            }

            "scroll" -> {
                val x = params?.optDouble("x")?.toFloat() ?: run {
                    sendResponse(ws, id, "error", error = "missing x param"); return
                }
                val y = params.optDouble("y").toFloat()
                val dx = params.optDouble("dx", 0.0).toFloat()
                val dy = params.optDouble("dy", 0.0).toFloat()
                val duration = params.optLong("duration", 300)
                service.scroll(x, y, dx, dy, duration, object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "ok")
                    }
                    override fun onCancelled(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "error", error = "gesture cancelled")
                    }
                })
            }

            "right_click", "middle_click", "mouse_scroll" -> {
                sendResponse(ws, id, "ok", JSONObject().put("unsupported", true))
            }

            "camera" -> {
                val cameraId = params?.optString("camera", "0") ?: "0"
                val quality = params?.optInt("quality", 80) ?: 80
                val maxWidth = if (params?.has("max_width") == true) params.optInt("max_width") else null
                val maxHeight = if (params?.has("max_height") == true) params.optInt("max_height") else null

                service.captureCamera(cameraId) { bitmap ->
                    if (bitmap == null) {
                        sendResponse(ws, id, "ok", JSONObject().put("image", ""))
                        return@captureCamera
                    }
                    try {
                        val scaled = service.scaleBitmap(bitmap, maxWidth, maxHeight)
                        val bytes = service.compressToWebP(scaled, quality)
                        scaled.recycle()
                        val base64 = Base64.encodeToString(bytes, Base64.NO_WRAP)
                        sendResponse(ws, id, "ok", JSONObject().put("image", base64))
                    } catch (e: Exception) {
                        sendResponse(ws, id, "error", error = e.message)
                    }
                }
            }

            else -> sendResponse(ws, id, "error", error = "unknown command: $cmd")
        }
    }

    private fun sendResponse(
        ws: WebSocket, id: Long, status: String,
        result: JSONObject? = null, error: String? = null
    ) {
        val response = JSONObject().apply {
            put("id", id)
            put("status", status)
            if (result != null) put("result", result)
            if (error != null) put("error", error)
        }
        ws.send(response.toString())
    }

    private fun scheduleReconnect() {
        if (!shouldReconnect.get()) return

        // Cancel idle timer during reconnection — don't let idle timeout kill the reconnect loop
        cancelIdleTimer()
        isConnecting.set(false)

        if (reconnectAttempt >= MAX_RECONNECT_ATTEMPTS) {
            Log.w(TAG, "Max reconnect attempts ($MAX_RECONNECT_ATTEMPTS) reached, giving up")
            shouldReconnect.set(false)
            handler.post { onStatusChange("Disconnected (max retries)") }
            return
        }

        val myGeneration = connectionGeneration
        val delay = minOf(
            (1000L * (1L shl minOf(reconnectAttempt, 5))),
            MAX_RECONNECT_DELAY_MS
        )
        reconnectAttempt++
        Log.i(TAG, "Reconnecting in ${delay}ms (attempt $reconnectAttempt/$MAX_RECONNECT_ATTEMPTS, gen=$myGeneration)")
        handler.post { onStatusChange("Reconnecting in ${delay / 1000}s (attempt $reconnectAttempt/$MAX_RECONNECT_ATTEMPTS)...") }

        val reconnectRunnable = Runnable {
            if (!shouldReconnect.get()) return@Runnable
            if (myGeneration != connectionGeneration) return@Runnable
            // If we have an API URL, rediscover (worker might have changed)
            if (apiUrl != null) {
                discoverAndConnect()
            } else {
                // Direct connection — retry same URL
                val url = lastWorkerUrl
                if (url != null) {
                    doConnect(url)
                } else {
                    Log.e(TAG, "No URL to reconnect to")
                    handler.post { onStatusChange("No worker URL") }
                }
            }
        }
        handler.postDelayed(reconnectRunnable, delay)
    }

    private fun cancelIdleTimer() {
        idleRunnable?.let { handler.removeCallbacks(it) }
        idleRunnable = null
    }

    private fun resetIdleTimer() {
        cancelIdleTimer()
        idleRunnable = Runnable {
            Log.i(TAG, "Idle timeout, disconnecting")
            disconnect()
        }
        handler.postDelayed(idleRunnable!!, IDLE_TIMEOUT_MS)
    }
}
