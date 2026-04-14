package com.doodkin.screenmcp

import android.accessibilityservice.AccessibilityService
import android.graphics.Bitmap
import android.hardware.camera2.CameraManager
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
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class WebSocketClient(
    private val onStatusChange: (String) -> Unit,
    private val onLog: ((String) -> Unit)? = null
) {
    companion object {
        private const val TAG = "WebSocketClient"
        private const val MAX_RECONNECT_DELAY_MS = 30_000L
        private const val MAX_RECONNECT_ATTEMPTS = 10
        const val DEFAULT_SCALE_WIDTH = 1456.0
        const val DEFAULT_SCALE_HEIGHT = 819.0
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

    /** Timestamp when the current connection sequence started */
    private var connectStartMs = 0L

    private val isConnected = AtomicBoolean(false)
    private val isConnecting = AtomicBoolean(false)
    private val shouldReconnect = AtomicBoolean(false)
    private var reconnectAttempt = 0
    private val handler = Handler(Looper.getMainLooper())
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

    /** Timestamped log: writes to logcat AND calls onLog callback for UI display */
    private fun tlog(msg: String) {
        val ts = SimpleDateFormat("HH:mm:ss.SSS", Locale.US).format(Date())
        Log.i(TAG, msg)
        onLog?.invoke("[$ts] $msg")
    }

    private fun discoverAndConnect() {
        val api = apiUrl ?: return
        val tok = token ?: return
        val myGeneration = connectionGeneration
        connectStartMs = System.currentTimeMillis()

        onStatusChange("Discovering worker...")
        tlog("Discovering worker via $api...")

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
                    tlog("Discovery failed: ${response.code} (${System.currentTimeMillis() - connectStartMs}ms)")
                    handler.post { onStatusChange("Discovery failed") }
                    scheduleReconnect()
                    return@Thread
                }

                val json = JSONObject(responseBody)
                val wsUrl = json.optString("wsUrl", "")
                if (wsUrl.isEmpty()) {
                    tlog("Discovery returned no wsUrl")
                    handler.post { onStatusChange("No worker available") }
                    scheduleReconnect()
                    return@Thread
                }

                tlog("Discovered worker in ${System.currentTimeMillis() - connectStartMs}ms: $wsUrl")
                lastWorkerUrl = wsUrl
                handler.post { doConnect(wsUrl) }
            } catch (e: Exception) {
                if (myGeneration != connectionGeneration) return@Thread
                tlog("Discovery error: ${e.message}")
                handler.post { onStatusChange("Discovery error") }
                scheduleReconnect()
            }
        }.start()
    }

    private fun doConnect(wsUrl: String) {
        val wsToken = token ?: return

        // Prevent concurrent connection attempts
        if (!isConnecting.compareAndSet(false, true)) {
            tlog("Already connecting, ignoring duplicate")
            return
        }

        val myGeneration = connectionGeneration
        val wsConnectStartMs = System.currentTimeMillis()

        onStatusChange("Connecting to $wsUrl...")
        tlog("WS connecting to $wsUrl...")

        val request = Request.Builder().url(wsUrl).build()

        webSocket = httpClient.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(ws: WebSocket, response: Response) {
                if (myGeneration != connectionGeneration) { ws.close(1000, "stale"); return }
                tlog("WS opened in ${System.currentTimeMillis() - wsConnectStartMs}ms, sending auth")
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
                tlog("WS closing: $code $reason")
                ws.close(code, reason)
            }

            override fun onClosed(ws: WebSocket, code: Int, reason: String) {
                tlog("WS closed: $code $reason")
                isConnected.set(false)
                isConnecting.set(false)
                if (myGeneration != connectionGeneration) return
                handler.post { onStatusChange("Disconnected") }
                scheduleReconnect()
            }

            override fun onFailure(ws: WebSocket, t: Throwable, response: Response?) {
                tlog("WS failure: ${t.message}")
                isConnected.set(false)
                isConnecting.set(false)
                if (myGeneration != connectionGeneration) return
                handler.post { onStatusChange("Connection failed: ${t.message}") }
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
                    val totalMs = if (connectStartMs > 0) System.currentTimeMillis() - connectStartMs else 0
                    tlog("Authenticated (total connect: ${totalMs}ms)")
                    isConnected.set(true)
                    isConnecting.set(false)
                    reconnectAttempt = 0
                    handler.post { onStatusChange("Connected") }
                }
                "auth_fail" -> {
                    tlog("Auth failed: ${json.optString("error")}")
                    isConnected.set(false)
                    shouldReconnect.set(false)
                    handler.post { onStatusChange("Auth failed") }
                }
                "ping" -> {
                    ws.send(JSONObject().put("type", "pong").toString())
                }
                "error" -> {
                    tlog("Server error: ${json.optString("error")}")
                }
                else -> {
                    if (json.has("id") && json.has("cmd")) {
                        executeCommand(ws, json)
                    }
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to handle message: ${e.message}")
        }
    }

    /** Scale coordinates from screenshot space to actual screen space using max_width/max_height params. */
    private fun scaleX(x: Double, params: JSONObject?, dm: android.util.DisplayMetrics): Float {
        val mw = params?.optDouble("max_width", DEFAULT_SCALE_WIDTH) ?: DEFAULT_SCALE_WIDTH
        if (mw > 0.0) return (x * dm.widthPixels / mw).toFloat()
        return x.toFloat()
    }

    private fun scaleY(y: Double, params: JSONObject?, dm: android.util.DisplayMetrics): Float {
        val mh = params?.optDouble("max_height", DEFAULT_SCALE_HEIGHT) ?: DEFAULT_SCALE_HEIGHT
        if (mh > 0.0) return (y * dm.heightPixels / mh).toFloat()
        return y.toFloat()
    }

    /** Get output scale factors (screen→screenshot space). Returns (sx, sy), both 1.0 if no scaling. */
    private fun getOutputScale(params: JSONObject?, dm: android.util.DisplayMetrics): Pair<Double, Double> {
        val mw = params?.optDouble("max_width", DEFAULT_SCALE_WIDTH) ?: DEFAULT_SCALE_WIDTH
        val mh = params?.optDouble("max_height", DEFAULT_SCALE_HEIGHT) ?: DEFAULT_SCALE_HEIGHT
        if (mw <= 0.0 && mh <= 0.0) return Pair(1.0, 1.0)
        val sx = if (mw > 0.0) mw / dm.widthPixels else mh / dm.heightPixels
        val sy = if (mh > 0.0) mh / dm.heightPixels else sx
        return Pair(sx, sy)
    }

    /** Recursively scale bounds fields in a JSONObject/JSONArray. */
    private fun scaleBoundsJson(value: Any, sx: Double, sy: Double): Any {
        return when (value) {
            is JSONObject -> {
                val out = JSONObject()
                for (key in value.keys()) {
                    val v = value.get(key)
                    val scaled = when (key) {
                        "left", "right", "x", "width" -> if (v is Number) Math.round(v.toDouble() * sx) else scaleBoundsJson(v, sx, sy)
                        "top", "bottom", "y", "height" -> if (v is Number) Math.round(v.toDouble() * sy) else scaleBoundsJson(v, sx, sy)
                        else -> scaleBoundsJson(v, sx, sy)
                    }
                    out.put(key, scaled)
                }
                out
            }
            is JSONArray -> {
                val out = JSONArray()
                for (i in 0 until value.length()) {
                    out.put(scaleBoundsJson(value.get(i), sx, sy))
                }
                out
            }
            else -> value
        }
    }

    private fun executeCommand(ws: WebSocket, json: JSONObject) {
        val id = json.getLong("id")
        val cmd = json.getString("cmd")
        val params = json.optJSONObject("params")
        val cmdStartMs = System.currentTimeMillis()

        tlog("CMD #$id $cmd received")

        val service = ScreenMcpService.instance
        if (service == null) {
            sendResponse(ws, id, "error", error = "accessibility service not connected")
            return
        }

        val dm = service.resources.displayMetrics

        when (cmd) {
            "screenshot" -> {
                if (service.isPhoneLocked()) {
                    sendResponse(ws, id, "error", error = "phone is locked")
                    return
                }
                val quality = params?.optInt("quality", 100) ?: 100
                val maxWidth = if (params?.has("max_width") == true) params.optInt("max_width") else DEFAULT_SCALE_WIDTH.toInt()
                val maxHeight = if (params?.has("max_height") == true) params.optInt("max_height") else DEFAULT_SCALE_HEIGHT.toInt()

                service.takeScreenshot(object : AccessibilityService.TakeScreenshotCallback {
                    override fun onSuccess(result: AccessibilityService.ScreenshotResult) {
                        try {
                            val captureMs = System.currentTimeMillis() - cmdStartMs
                            tlog("CMD #$id captured (${captureMs}ms)")

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
                            val totalMs = System.currentTimeMillis() - cmdStartMs
                            tlog("CMD #$id done (${totalMs}ms, ${bytes.size / 1024}KB)")
                            sendResponse(ws, id, "ok", JSONObject().put("image", base64))
                        } catch (e: Exception) {
                            tlog("CMD #$id error: ${e.message} (${System.currentTimeMillis() - cmdStartMs}ms)")
                            sendResponse(ws, id, "error", error = e.message)
                        }
                    }

                    override fun onFailure(errorCode: Int) {
                        tlog("CMD #$id screenshot failed: $errorCode (${System.currentTimeMillis() - cmdStartMs}ms)")
                        sendResponse(ws, id, "error", error = "screenshot failed: $errorCode")
                    }
                })
            }

            "ui_tree" -> {
                var tree: Any = service.getUiTreeJson()
                val (sx, sy) = getOutputScale(params, dm)
                if (sx != 1.0 || sy != 1.0) tree = scaleBoundsJson(tree, sx, sy)
                sendResponse(ws, id, "ok", JSONObject().put("tree", tree).put("os", "android"))
            }

            "click" -> {
                val rawX = params?.optDouble("x") ?: run {
                    sendResponse(ws, id, "error", error = "missing x param"); return
                }
                val rawY = params.optDouble("y")
                val x = scaleX(rawX, params, dm)
                val y = scaleY(rawY, params, dm)
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
                val rawX = params?.optDouble("x") ?: run {
                    sendResponse(ws, id, "error", error = "missing x param"); return
                }
                val rawY = params.optDouble("y")
                val x = scaleX(rawX, params, dm)
                val y = scaleY(rawY, params, dm)
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
                val rawSx = params?.optDouble("startX") ?: run {
                    sendResponse(ws, id, "error", error = "missing startX param"); return
                }
                val sx = scaleX(rawSx, params, dm)
                val sy = scaleY(params.optDouble("startY"), params, dm)
                val ex = scaleX(params.optDouble("endX"), params, dm)
                val ey = scaleY(params.optDouble("endY"), params, dm)
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
                if (!ok) {
                    sendResponse(ws, id, "error", error = "copy failed")
                } else {
                    val returnText = params?.optBoolean("return_text", false) ?: false
                    if (returnText) {
                        val text = service.getClipboard() ?: ""
                        sendResponse(ws, id, "ok", JSONObject().put("text", text))
                    } else {
                        sendResponse(ws, id, "ok")
                    }
                }
            }
            "paste" -> {
                val text = params?.optString("text", "") ?: ""
                if (text.isNotEmpty()) {
                    service.setClipboard(text)
                }
                val ok = service.paste()
                sendResponse(ws, id, if (ok) "ok" else "error",
                    error = if (!ok) "paste failed" else null)
            }
            "get_clipboard" -> {
                val text = service.getClipboard()
                sendResponse(ws, id, "ok", JSONObject().put("text", text ?: ""))
            }
            "set_clipboard" -> {
                val text = params?.optString("text", "") ?: ""
                service.setClipboard(text)
                sendResponse(ws, id, "ok")
            }

            "scroll" -> {
                val rawX = params?.optDouble("x") ?: run {
                    sendResponse(ws, id, "error", error = "missing x param"); return
                }
                val x = scaleX(rawX, params, dm)
                val y = scaleY(params.optDouble("y"), params, dm)
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
                val response = JSONObject().apply {
                    put("id", id)
                    put("status", "ok")
                    put("unsupported", true)
                }
                ws.send(response.toString())
            }

            "list_cameras" -> {
                val cameraManager = service.getSystemService(android.content.Context.CAMERA_SERVICE) as CameraManager
                val cameras = org.json.JSONArray()
                try {
                    for (camId in cameraManager.cameraIdList) {
                        val characteristics = cameraManager.getCameraCharacteristics(camId)
                        val lensFacing = characteristics.get(android.hardware.camera2.CameraCharacteristics.LENS_FACING)
                        val facing = when (lensFacing) {
                            android.hardware.camera2.CameraCharacteristics.LENS_FACING_BACK -> "back"
                            android.hardware.camera2.CameraCharacteristics.LENS_FACING_FRONT -> "front"
                            android.hardware.camera2.CameraCharacteristics.LENS_FACING_EXTERNAL -> "external"
                            else -> "unknown"
                        }
                        cameras.put(JSONObject().put("id", camId).put("facing", facing))
                    }
                } catch (e: Exception) {
                    Log.e(TAG, "Failed to list cameras: ${e.message}")
                }
                sendResponse(ws, id, "ok", JSONObject().put("cameras", cameras))
            }

            "camera" -> {
                val cameraId = params?.optString("camera", "0") ?: "0"
                val quality = params?.optInt("quality", 80) ?: 80
                val maxWidth = if (params?.has("max_width") == true) params.optInt("max_width") else DEFAULT_SCALE_WIDTH.toInt()
                val maxHeight = if (params?.has("max_height") == true) params.optInt("max_height") else DEFAULT_SCALE_HEIGHT.toInt()

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

            "play_audio" -> {
                val audioData = params?.optString("audio_data", "") ?: ""
                if (audioData.isEmpty()) {
                    sendResponse(ws, id, "error", error = "missing audio_data param")
                    return
                }
                val volume = params?.optDouble("volume", 1.0)?.toFloat() ?: 1.0f
                val clampedVolume = volume.coerceIn(0.0f, 1.0f)

                service.playAudio(audioData, clampedVolume) { success, errorMsg ->
                    if (success) {
                        val totalMs = System.currentTimeMillis() - cmdStartMs
                        tlog("CMD #$id play_audio done (${totalMs}ms)")
                        sendResponse(ws, id, "ok")
                    } else {
                        tlog("CMD #$id play_audio failed: $errorMsg")
                        sendResponse(ws, id, "error", error = errorMsg)
                    }
                }
            }

            "get_screen_size" -> {
                val ow = dm.widthPixels
                val oh = dm.heightPixels
                val mw = params?.optDouble("max_width", DEFAULT_SCALE_WIDTH) ?: DEFAULT_SCALE_WIDTH
                val mh = params?.optDouble("max_height", DEFAULT_SCALE_HEIGHT) ?: DEFAULT_SCALE_HEIGHT
                val result = JSONObject()
                if (mw > 0.0 || mh > 0.0) {
                    val r = when {
                        mw > 0.0 && mh > 0.0 -> minOf(mw / ow, mh / oh, 1.0)
                        mw > 0.0 -> minOf(mw / ow, 1.0)
                        else -> minOf(mh / oh, 1.0)
                    }
                    result.put("width", (ow * r).toInt())
                    result.put("height", (oh * r).toInt())
                    result.put("original_width", ow)
                    result.put("original_height", oh)
                    result.put("scaled", true)
                } else {
                    result.put("width", ow)
                    result.put("height", oh)
                }
                result.put("density", dm.density.toDouble())
                sendResponse(ws, id, "ok", result = result)
            }

            "double_click" -> {
                val rawX = params?.optDouble("x", -1.0) ?: -1.0
                val rawY = params?.optDouble("y", -1.0) ?: -1.0
                if (rawX < 0 || rawY < 0) {
                    sendResponse(ws, id, "error", error = "missing x or y")
                    return
                }
                val x = scaleX(rawX, params, dm)
                val y = scaleY(rawY, params, dm)
                // Two quick taps
                service.click(x, y, 50, object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(g: android.accessibilityservice.GestureDescription?) {
                        handler.postDelayed({
                            service.click(x, y, 50, object : AccessibilityService.GestureResultCallback() {
                                override fun onCompleted(g: android.accessibilityservice.GestureDescription?) {
                                    sendResponse(ws, id, "ok")
                                }
                                override fun onCancelled(g: android.accessibilityservice.GestureDescription?) {
                                    sendResponse(ws, id, "error", error = "second tap cancelled")
                                }
                            })
                        }, 50)
                    }
                    override fun onCancelled(g: android.accessibilityservice.GestureDescription?) {
                        sendResponse(ws, id, "error", error = "first tap cancelled")
                    }
                })
            }

            "list_windows" -> {
                var windows: Any = service.getWindowList()
                val (sx, sy) = getOutputScale(params, dm)
                if (sx != 1.0 || sy != 1.0) windows = scaleBoundsJson(windows, sx, sy)
                sendResponse(ws, id, "ok", result = JSONObject().put("windows", windows))
            }

            "active_window" -> {
                var result = service.getActiveWindow()
                val (sx, sy) = getOutputScale(params, dm)
                if (sx != 1.0 || sy != 1.0) result = scaleBoundsJson(result, sx, sy) as JSONObject
                sendResponse(ws, id, "ok", result = result)
            }

            "focus_window" -> {
                val title = params?.optString("title", "") ?: ""
                if (title.isEmpty()) {
                    sendResponse(ws, id, "error", error = "missing title param")
                    return
                }
                // Launch app by name using package manager search
                try {
                    val pm = service.packageManager
                    val intent = android.content.Intent(android.content.Intent.ACTION_MAIN)
                    intent.addCategory(android.content.Intent.CATEGORY_LAUNCHER)
                    val apps = pm.queryIntentActivities(intent, 0)
                    val match = apps.find {
                        it.loadLabel(pm).toString().contains(title, ignoreCase = true)
                    }
                    if (match != null) {
                        val launchIntent = pm.getLaunchIntentForPackage(match.activityInfo.packageName)
                        if (launchIntent != null) {
                            launchIntent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                            service.startActivity(launchIntent)
                            sendResponse(ws, id, "ok", result = JSONObject().put("focused", match.loadLabel(pm).toString()))
                        } else {
                            sendResponse(ws, id, "error", error = "cannot launch ${match.loadLabel(pm)}")
                        }
                    } else {
                        sendResponse(ws, id, "error", error = "no app matching '$title'")
                    }
                } catch (e: Exception) {
                    sendResponse(ws, id, "error", error = "focus_window failed: ${e.message}")
                }
            }

            "screenshot_region" -> {
                if (service.isPhoneLocked()) {
                    sendResponse(ws, id, "error", error = "phone is locked")
                    return
                }
                val minX = params?.optDouble("min_x", Double.NaN) ?: Double.NaN
                val minY = params?.optDouble("min_y", Double.NaN) ?: Double.NaN
                val maxX = params?.optDouble("max_x", Double.NaN) ?: Double.NaN
                val maxY = params?.optDouble("max_y", Double.NaN) ?: Double.NaN
                if (minX.isNaN() || minY.isNaN() || maxX.isNaN() || maxY.isNaN()) {
                    sendResponse(ws, id, "error", error = "missing min_x/min_y/max_x/max_y params")
                    return
                }
                val quality = params?.optInt("quality", 100) ?: 100

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
                            val softBitmap = bitmap.copy(Bitmap.Config.ARGB_8888, false)
                            bitmap.recycle()
                            hwBuffer.close()

                            val fullW = softBitmap.width.toDouble()
                            val fullH = softBitmap.height.toDouble()

                            // Scale from screenshot space to actual pixels
                            val mw = params?.optDouble("max_width", DEFAULT_SCALE_WIDTH) ?: DEFAULT_SCALE_WIDTH
                            val mh = params?.optDouble("max_height", DEFAULT_SCALE_HEIGHT) ?: DEFAULT_SCALE_HEIGHT
                            val sx = if (mw > 0) fullW / mw else 1.0
                            val sy = if (mh > 0) fullH / mh else 1.0

                            val pxMinX = (minX * sx).toInt().coerceIn(0, softBitmap.width - 1)
                            val pxMinY = (minY * sy).toInt().coerceIn(0, softBitmap.height - 1)
                            val pxMaxX = (maxX * sx).toInt().coerceIn(0, softBitmap.width)
                            val pxMaxY = (maxY * sy).toInt().coerceIn(0, softBitmap.height)

                            val cropW = pxMaxX - pxMinX
                            val cropH = pxMaxY - pxMinY
                            if (cropW <= 0 || cropH <= 0) {
                                softBitmap.recycle()
                                sendResponse(ws, id, "error", error = "region has zero or negative size")
                                return
                            }

                            var cropped = Bitmap.createBitmap(softBitmap, pxMinX, pxMinY, cropW, cropH)
                            softBitmap.recycle()

                            // Only scale down if output_max specified
                            val outMaxW = params?.optInt("output_max_width", 0) ?: 0
                            val outMaxH = params?.optInt("output_max_height", 0) ?: 0
                            if (outMaxW > 0 || outMaxH > 0) {
                                cropped = service.scaleBitmap(cropped,
                                    if (outMaxW > 0) outMaxW else null,
                                    if (outMaxH > 0) outMaxH else null)
                            }

                            val bytes = service.compressToWebP(cropped, quality)
                            cropped.recycle()

                            val base64 = Base64.encodeToString(bytes, Base64.NO_WRAP)
                            sendResponse(ws, id, "ok", JSONObject().put("image", base64))
                        } catch (e: Exception) {
                            sendResponse(ws, id, "error", error = "screenshot_region failed: ${e.message}")
                        }
                    }

                    override fun onFailure(errorCode: Int) {
                        sendResponse(ws, id, "error", error = "screenshot failed: $errorCode")
                    }
                })
            }

            "hold_key", "release_key", "press_key", "hotkey", "mouse_move",
            "screenshot_window", "is_elevated", "elevate" -> {
                sendResponse(ws, id, "ok", result = JSONObject().put("unsupported", true))
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
        tlog("Reconnecting in ${delay}ms (attempt $reconnectAttempt/$MAX_RECONNECT_ATTEMPTS)")
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

}
