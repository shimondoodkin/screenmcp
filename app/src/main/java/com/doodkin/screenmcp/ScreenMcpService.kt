package com.doodkin.screenmcp

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.app.KeyguardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageFormat
import android.graphics.Path
import android.graphics.Rect
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.media.ImageReader
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream

class ScreenMcpService : AccessibilityService() {

    companion object {
        private const val TAG = "ScreenMcpService"

        var instance: ScreenMcpService? = null
            private set

        val isConnected: Boolean
            get() = instance != null
    }

    private var wsClient: WebSocketClient? = null
    private var connectionStatus = "Disconnected"

    /** Listeners for status changes (ConnectionService registers here for notifications) */
    var onConnectionStatusChange: ((String) -> Unit)? = null

    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
        Log.i(TAG, "Accessibility service connected")
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        // Minimal handling
    }

    override fun onInterrupt() {}

    override fun onDestroy() {
        super.onDestroy()
        disconnectWorker()
        instance = null
        Log.i(TAG, "Accessibility service destroyed")
    }

    // --- WebSocket Connection Management ---

    fun getConnectionStatus(): String = connectionStatus

    fun isWorkerConnected(): Boolean = wsClient?.isConnected() == true

    fun isWorkerConnectedTo(url: String): Boolean = wsClient?.isConnectedTo(url) == true

    fun connectViaApi(apiUrl: String, token: String) {
        ensureClient()
        wsClient?.connectViaApi(apiUrl, token)
    }

    fun connectDirect(workerUrl: String, token: String, fallbackApiUrl: String? = null) {
        ensureClient()
        wsClient?.connectDirect(workerUrl, token, fallbackApiUrl)
    }

    fun disconnectWorker() {
        wsClient?.disconnect()
    }

    private fun ensureClient() {
        if (wsClient == null) {
            wsClient = WebSocketClient { status ->
                connectionStatus = status
                Log.i(TAG, "Worker status: $status")
                onConnectionStatusChange?.invoke(status)
            }
        }
    }

    // --- Gesture Actions ---

    fun click(x: Float, y: Float, durationMs: Long = 100, callback: GestureResultCallback? = null) {
        val path = Path().apply { moveTo(x, y) }
        val stroke = GestureDescription.StrokeDescription(path, 0, durationMs)
        val gesture = GestureDescription.Builder().addStroke(stroke).build()
        dispatchGesture(gesture, callback, null)
    }

    fun longClick(x: Float, y: Float, callback: GestureResultCallback? = null) {
        val path = Path().apply { moveTo(x, y) }
        val stroke = GestureDescription.StrokeDescription(path, 0, 1000)
        val gesture = GestureDescription.Builder().addStroke(stroke).build()
        dispatchGesture(gesture, callback, null)
    }

    fun drag(
        startX: Float, startY: Float,
        endX: Float, endY: Float,
        durationMs: Long = 300,
        callback: GestureResultCallback? = null
    ) {
        val path = Path().apply {
            moveTo(startX, startY)
            lineTo(endX, endY)
        }
        val stroke = GestureDescription.StrokeDescription(path, 0, durationMs)
        val gesture = GestureDescription.Builder().addStroke(stroke).build()
        dispatchGesture(gesture, callback, null)
    }

    // --- Text Actions ---

    fun getTextFromFocused(): String? {
        val rootNode = rootInActiveWindow ?: return null
        val target = rootNode.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            ?: rootNode.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)
            ?: findEditableNode(rootNode)
            ?: return null
        return target.text?.toString()
    }

    fun typeText(text: String): Boolean {
        val rootNode = rootInActiveWindow ?: return false
        val target = rootNode.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
            ?: rootNode.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY)
            ?: findEditableNode(rootNode)
            ?: return false
        val existing = target.text?.toString() ?: ""
        val args = Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, existing + text)
        }
        return target.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)
    }

    private fun findEditableNode(node: AccessibilityNodeInfo): AccessibilityNodeInfo? {
        if (node.isFocused && node.isEditable) return node
        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            val result = findEditableNode(child)
            if (result != null) return result
        }
        return null
    }

    fun selectAll(): Boolean {
        val rootNode = rootInActiveWindow ?: return false
        val focusedNode = rootNode.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return false
        val text = focusedNode.text ?: return false
        val args = Bundle().apply {
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, 0)
            putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, text.length)
        }
        return focusedNode.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, args)
    }

    fun copy(): Boolean {
        val rootNode = rootInActiveWindow ?: return false
        val focusedNode = rootNode.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return false
        return focusedNode.performAction(AccessibilityNodeInfo.ACTION_COPY)
    }

    fun paste(): Boolean {
        val rootNode = rootInActiveWindow ?: return false
        val focusedNode = rootNode.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return false
        return focusedNode.performAction(AccessibilityNodeInfo.ACTION_PASTE)
    }

    // --- UI Tree ---

    fun getUiTree(): String {
        val rootNode = rootInActiveWindow ?: return "(no active window)"
        val sb = StringBuilder()
        dumpNode(rootNode, sb, 0)
        return sb.toString()
    }

    private fun dumpNode(node: AccessibilityNodeInfo, sb: StringBuilder, depth: Int) {
        val indent = "  ".repeat(depth)
        val cls = node.className?.toString()?.substringAfterLast('.') ?: "?"
        val text = node.text?.toString()?.take(50) ?: ""
        val desc = node.contentDescription?.toString()?.take(50) ?: ""
        val id = node.viewIdResourceName ?: ""
        val flags = buildList {
            if (node.isClickable) add("click")
            if (node.isEditable) add("edit")
            if (node.isFocused) add("focused")
            if (node.isScrollable) add("scroll")
            if (node.isCheckable) add("check")
            if (node.isChecked) add("checked")
        }.joinToString(",")

        sb.append(indent).append(cls)
        if (id.isNotEmpty()) sb.append(" id=").append(id.substringAfterLast('/'))
        if (text.isNotEmpty()) sb.append(" text=\"").append(text).append("\"")
        if (desc.isNotEmpty()) sb.append(" desc=\"").append(desc).append("\"")
        if (flags.isNotEmpty()) sb.append(" [").append(flags).append("]")
        val bounds = android.graphics.Rect()
        node.getBoundsInScreen(bounds)
        sb.append(" (").append(bounds.toShortString()).append(")")
        sb.append("\n")

        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            dumpNode(child, sb, depth + 1)
        }
    }

    // --- Structured UI Tree (JSON) ---

    fun getUiTreeJson(): JSONArray {
        val rootNode = rootInActiveWindow ?: return JSONArray()
        val result = JSONArray()
        dumpNodeJson(rootNode, result)
        return result
    }

    private fun dumpNodeJson(node: AccessibilityNodeInfo, arr: JSONArray) {
        val obj = JSONObject()
        obj.put("className", node.className?.toString()?.substringAfterLast('.') ?: "")
        obj.put("text", node.text?.toString() ?: "")
        obj.put("contentDescription", node.contentDescription?.toString() ?: "")
        obj.put("resourceId", node.viewIdResourceName ?: "")
        obj.put("clickable", node.isClickable)
        obj.put("editable", node.isEditable)
        obj.put("focused", node.isFocused)
        obj.put("scrollable", node.isScrollable)
        obj.put("checkable", node.isCheckable)
        obj.put("checked", node.isChecked)

        val bounds = Rect()
        node.getBoundsInScreen(bounds)
        obj.put("bounds", JSONObject().apply {
            put("left", bounds.left)
            put("top", bounds.top)
            put("right", bounds.right)
            put("bottom", bounds.bottom)
        })

        if (node.childCount > 0) {
            val children = JSONArray()
            for (i in 0 until node.childCount) {
                val child = node.getChild(i) ?: continue
                dumpNodeJson(child, children)
            }
            obj.put("children", children)
        }

        arr.put(obj)
    }

    // --- Global Actions ---

    fun pressBack(): Boolean = performGlobalAction(GLOBAL_ACTION_BACK)
    fun pressHome(): Boolean = performGlobalAction(GLOBAL_ACTION_HOME)
    fun pressRecents(): Boolean = performGlobalAction(GLOBAL_ACTION_RECENTS)

    // --- Phone Lock Detection ---

    fun isPhoneLocked(): Boolean {
        val km = getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        return km.isKeyguardLocked
    }

    // --- Screenshot ---

    fun takeScreenshot(callback: TakeScreenshotCallback) {
        takeScreenshot(
            android.view.Display.DEFAULT_DISPLAY,
            mainExecutor,
            callback
        )
    }

    // --- Scroll (finger drag gesture) ---

    fun scroll(
        x: Float, y: Float, dx: Float, dy: Float,
        durationMs: Long = 300,
        callback: GestureResultCallback? = null
    ) {
        val path = Path().apply {
            moveTo(x, y)
            lineTo(x + dx, y + dy)
        }
        val stroke = GestureDescription.StrokeDescription(path, 0, durationMs)
        val gesture = GestureDescription.Builder().addStroke(stroke).build()
        dispatchGesture(gesture, callback, null)
    }

    // --- Camera Capture ---

    fun captureCamera(cameraId: String, callback: (Bitmap?) -> Unit) {
        val cameraManager = getSystemService(Context.CAMERA_SERVICE) as CameraManager
        val cameraIds = cameraManager.cameraIdList
        if (!cameraIds.contains(cameraId)) {
            callback(null)
            return
        }

        val handlerThread = HandlerThread("CameraCapture").apply { start() }
        val cameraHandler = Handler(handlerThread.looper)

        val imageReader = ImageReader.newInstance(1920, 1080, ImageFormat.JPEG, 1)

        try {
            cameraManager.openCamera(cameraId, object : CameraDevice.StateCallback() {
                override fun onOpened(camera: CameraDevice) {
                    try {
                        val captureBuilder = camera.createCaptureRequest(CameraDevice.TEMPLATE_STILL_CAPTURE)
                        captureBuilder.addTarget(imageReader.surface)
                        captureBuilder.set(CaptureRequest.CONTROL_AF_MODE, CaptureRequest.CONTROL_AF_MODE_AUTO)
                        captureBuilder.set(CaptureRequest.JPEG_ORIENTATION, 0)

                        camera.createCaptureSession(
                            listOf(imageReader.surface),
                            object : CameraCaptureSession.StateCallback() {
                                override fun onConfigured(session: CameraCaptureSession) {
                                    imageReader.setOnImageAvailableListener({ reader ->
                                        val image = reader.acquireLatestImage()
                                        if (image != null) {
                                            val buffer = image.planes[0].buffer
                                            val bytes = ByteArray(buffer.remaining())
                                            buffer.get(bytes)
                                            image.close()
                                            val bitmap = android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                                            session.close()
                                            camera.close()
                                            handlerThread.quitSafely()
                                            callback(bitmap)
                                        } else {
                                            session.close()
                                            camera.close()
                                            handlerThread.quitSafely()
                                            callback(null)
                                        }
                                    }, cameraHandler)

                                    session.capture(captureBuilder.build(), null, cameraHandler)
                                }

                                override fun onConfigureFailed(session: CameraCaptureSession) {
                                    camera.close()
                                    handlerThread.quitSafely()
                                    callback(null)
                                }
                            },
                            cameraHandler
                        )
                    } catch (e: Exception) {
                        Log.e(TAG, "Camera capture error: ${e.message}")
                        camera.close()
                        handlerThread.quitSafely()
                        callback(null)
                    }
                }

                override fun onDisconnected(camera: CameraDevice) {
                    camera.close()
                    handlerThread.quitSafely()
                    callback(null)
                }

                override fun onError(camera: CameraDevice, error: Int) {
                    Log.e(TAG, "Camera open error: $error")
                    camera.close()
                    handlerThread.quitSafely()
                    callback(null)
                }
            }, cameraHandler)
        } catch (e: SecurityException) {
            Log.e(TAG, "Camera permission denied: ${e.message}")
            handlerThread.quitSafely()
            callback(null)
        } catch (e: Exception) {
            Log.e(TAG, "Camera error: ${e.message}")
            handlerThread.quitSafely()
            callback(null)
        }
    }

    // --- Image Helpers ---

    fun scaleBitmap(bitmap: Bitmap, maxWidth: Int?, maxHeight: Int?): Bitmap {
        if (maxWidth == null && maxHeight == null) return bitmap
        val origW = bitmap.width
        val origH = bitmap.height
        var scale = 1.0f
        if (maxWidth != null && origW > maxWidth) {
            scale = minOf(scale, maxWidth.toFloat() / origW)
        }
        if (maxHeight != null && origH > maxHeight) {
            scale = minOf(scale, maxHeight.toFloat() / origH)
        }
        if (scale >= 1.0f) return bitmap
        val newW = (origW * scale).toInt()
        val newH = (origH * scale).toInt()
        val scaled = Bitmap.createScaledBitmap(bitmap, newW, newH, true)
        if (scaled !== bitmap) bitmap.recycle()
        return scaled
    }

    fun compressToWebP(bitmap: Bitmap, quality: Int): ByteArray {
        val baos = ByteArrayOutputStream()
        if (quality >= 100) {
            bitmap.compress(Bitmap.CompressFormat.WEBP_LOSSLESS, 100, baos)
        } else {
            bitmap.compress(Bitmap.CompressFormat.WEBP_LOSSY, quality, baos)
        }
        return baos.toByteArray()
    }
}
