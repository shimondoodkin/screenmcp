package com.doodkin.screenmcp

import android.Manifest
import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.BitmapFactory
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.messaging.FirebaseMessaging
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.security.SecureRandom

class MainActivity : AppCompatActivity() {

    private lateinit var tvStatus: TextView
    private lateinit var tvLog: TextView
    private lateinit var tvConnectionStatus: TextView
    private lateinit var tvRegistrationStatus: TextView
    private lateinit var layoutRegister: LinearLayout
    private lateinit var btnRegister: Button
    private lateinit var btnUnregister: Button
    private lateinit var ivScreenshot: ImageView
    private lateinit var screenshotManager: ScreenshotManager
    private var isRegistered = false

    private val handler = Handler(Looper.getMainLooper())
    private val httpClient = OkHttpClient()
    private var lastLogVersion = 0L
    private val statusChecker = object : Runnable {
        override fun run() {
            updateServiceStatus()
            updateConnectionStatus()
            updateWorkerLog()
            handler.postDelayed(this, 1000)
        }
    }

    private fun isOpenSourceMode(): Boolean {
        val prefs = getSharedPreferences("screenmcp", MODE_PRIVATE)
        return prefs.getBoolean("opensource_server_enabled", false)
    }

    private fun isUseSse(): Boolean {
        val prefs = getSharedPreferences("screenmcp", MODE_PRIVATE)
        return prefs.getBoolean("use_sse", false)
    }

    /** Start SSE service using Firebase ID token (for Firebase + SSE mode) */
    private fun startSseWithFirebaseToken() {
        val user = FirebaseAuth.getInstance().currentUser ?: return
        user.getIdToken(false).addOnSuccessListener { result ->
            val token = result.token ?: return@addOnSuccessListener
            val apiUrl = getApiUrl()
            val intent = android.content.Intent(this, SseService::class.java).apply {
                putExtra("user_id", token)
                putExtra("api_url", apiUrl)
                putExtra("firebase_mode", true)
            }
            startForegroundService(intent)
        }
    }

    private fun getOpenSourceUserId(): String {
        val prefs = getSharedPreferences("screenmcp", MODE_PRIVATE)
        return prefs.getString("opensource_user_id", "") ?: ""
    }

    private fun getOpenSourceApiUrl(): String {
        val prefs = getSharedPreferences("screenmcp", MODE_PRIVATE)
        return prefs.getString("opensource_api_url", "") ?: ""
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        screenshotManager = ScreenshotManager(cacheDir)

        tvStatus = findViewById(R.id.tvStatus)
        tvLog = findViewById(R.id.tvLog)
        tvConnectionStatus = findViewById(R.id.tvConnectionStatus)
        tvRegistrationStatus = findViewById(R.id.tvRegistrationStatus)
        layoutRegister = findViewById(R.id.layoutRegister)
        ivScreenshot = findViewById(R.id.ivScreenshot)

        setupUserInfo()
        setupAccessibilityButton()
        setupPermissionsButton()
        setupRegistration()
        setupScreenshotButton()
        setupClickButton()
        setupDragButton()
        setupTypeButton()
        setupClipboardButtons()
        setupGlobalActionButtons()
        setupCameraButtons()
        setupUiTreeButton()

        if (isOpenSourceMode()) {
            // In open source mode, hide registration section
            tvRegistrationStatus.visibility = View.GONE
            layoutRegister.visibility = View.GONE

            // Start SSE service
            SseService.start(this)
        } else {
            // Firebase mode
            if (isUseSse()) {
                startSseWithFirebaseToken()
            }
            // Check registration on load
            checkRegistration()
        }
    }

    override fun onResume() {
        super.onResume()
        handler.post(statusChecker)
        updatePermissionsButton(findViewById(R.id.btnEnablePermissions))
    }

    override fun onPause() {
        super.onPause()
        handler.removeCallbacks(statusChecker)
    }

    private fun getApiUrl(): String {
        if (isOpenSourceMode()) {
            return getOpenSourceApiUrl()
        }
        return "https://screenmcp.com"
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

    private fun updateServiceStatus() {
        if (ScreenMcpService.isConnected) {
            tvStatus.text = "Service: Connected"
            tvStatus.setBackgroundColor(0xFFC8E6C9.toInt())
        } else {
            tvStatus.text = "Service: Disconnected"
            tvStatus.setBackgroundColor(0xFFFFCDD2.toInt())
        }
    }

    private fun updateConnectionStatus() {
        val svc = ConnectionService.instance
        if (svc != null) {
            val status = svc.getStatus()
            tvConnectionStatus.text = "Worker: $status"
            if (svc.isConnected()) {
                tvConnectionStatus.setBackgroundColor(0xFFC8E6C9.toInt()) // green
            } else if (status.contains("Disconnected", ignoreCase = true) ||
                       status.contains("failed", ignoreCase = true) ||
                       status.contains("Auth failed", ignoreCase = true)) {
                tvConnectionStatus.setBackgroundColor(0xFFFFCDD2.toInt()) // red
            } else {
                tvConnectionStatus.setBackgroundColor(0xFFFFF9C4.toInt()) // yellow = connecting/reconnecting
            }
        } else {
            tvConnectionStatus.text = "Worker: Not started"
            tvConnectionStatus.setBackgroundColor(0xFFFFCDD2.toInt())
        }
    }

    /** Pull timing logs from ConnectionService into the log window */
    private fun updateWorkerLog() {
        val svc = ConnectionService.instance ?: return
        if (svc.logVersion != lastLogVersion) {
            lastLogVersion = svc.logVersion
            val entries = svc.getLogEntries()
            tvLog.text = entries.takeLast(30).joinToString("\n")
        }
    }

    private fun setupUserInfo() {
        val tvUser = findViewById<TextView>(R.id.tvUser)
        val btnSignOut = findViewById<Button>(R.id.btnSignOut)

        if (isOpenSourceMode()) {
            tvUser.text = "Open Source Mode (${getOpenSourceUserId()})"
            btnSignOut.text = "Disable"
            btnSignOut.setOnClickListener {
                // Stop SSE service
                SseService.stop(this)
                // Disconnect worker
                ConnectionService.instance?.disconnect()
                // Disable open source mode
                val prefs = getSharedPreferences("screenmcp", MODE_PRIVATE)
                prefs.edit().putBoolean("opensource_server_enabled", false).apply()
                // Go back to login
                startActivity(Intent(this, LoginActivity::class.java))
                finish()
            }
        } else {
            val user = FirebaseAuth.getInstance().currentUser
            tvUser.text = user?.email ?: user?.displayName ?: "Not signed in"
            btnSignOut.setOnClickListener {
                ConnectionService.instance?.disconnect()
                FirebaseAuth.getInstance().signOut()
                startActivity(Intent(this, LoginActivity::class.java))
                finish()
            }
        }
    }

    private fun requireService(): ScreenMcpService? {
        val svc = ScreenMcpService.instance
        if (svc == null) {
            log("Service not connected. Enable it in Accessibility Settings.")
        }
        return svc
    }

    private fun setupAccessibilityButton() {
        findViewById<Button>(R.id.btnOpenAccessibility).setOnClickListener {
            startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
        }
    }

    companion object {
        private const val PERMISSIONS_REQUEST_CODE = 1001
    }

    private fun setupPermissionsButton() {
        val btn = findViewById<Button>(R.id.btnEnablePermissions)
        updatePermissionsButton(btn)
        btn.setOnClickListener {
            requestMissingPermissions()
        }
    }

    private fun getMissingPermissions(): List<String> {
        val needed = mutableListOf<String>()
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA)
            != PackageManager.PERMISSION_GRANTED) {
            needed.add(Manifest.permission.CAMERA)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED) {
                needed.add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        return needed
    }

    private fun updatePermissionsButton(btn: Button) {
        val missing = getMissingPermissions()
        if (missing.isEmpty()) {
            btn.text = "Permissions: All Granted"
            btn.isEnabled = false
        } else {
            val names = missing.map {
                when (it) {
                    Manifest.permission.CAMERA -> "Camera"
                    Manifest.permission.POST_NOTIFICATIONS -> "Notifications"
                    else -> it
                }
            }
            btn.text = "Enable Permissions (${names.joinToString(", ")})"
            btn.isEnabled = true
        }
    }

    private fun requestMissingPermissions() {
        val missing = getMissingPermissions()
        if (missing.isEmpty()) {
            log("All permissions already granted")
            return
        }
        ActivityCompat.requestPermissions(this, missing.toTypedArray(), PERMISSIONS_REQUEST_CODE)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == PERMISSIONS_REQUEST_CODE) {
            for (i in permissions.indices) {
                val name = when (permissions[i]) {
                    Manifest.permission.CAMERA -> "Camera"
                    Manifest.permission.POST_NOTIFICATIONS -> "Notifications"
                    else -> permissions[i]
                }
                if (grantResults[i] == PackageManager.PERMISSION_GRANTED) {
                    log("$name permission granted")
                } else {
                    log("$name permission denied")
                }
            }
            updatePermissionsButton(findViewById(R.id.btnEnablePermissions))
        }
    }

    // --- Registration ---

    private fun checkRegistration() {
        if (isOpenSourceMode()) {
            // No registration check needed in open source mode
            tvRegistrationStatus.visibility = View.GONE
            layoutRegister.visibility = View.GONE
            return
        }

        val user = FirebaseAuth.getInstance().currentUser
        if (user == null) {
            tvRegistrationStatus.text = "Not signed in"
            tvRegistrationStatus.setBackgroundColor(0xFFFFCDD2.toInt())
            layoutRegister.visibility = View.VISIBLE
            return
        }

        tvRegistrationStatus.text = "Checking registration..."
        tvRegistrationStatus.setBackgroundColor(0xFFFFF9C4.toInt())

        user.getIdToken(false).addOnSuccessListener { result ->
            val token = result.token ?: return@addOnSuccessListener
            val apiUrl = getApiUrl()

            Thread {
                try {
                    val request = Request.Builder()
                        .url("$apiUrl/api/devices/status")
                        .addHeader("Authorization", "Bearer $token")
                        .build()
                    val response = httpClient.newCall(request).execute()
                    val body = response.body?.string() ?: "{}"
                    val json = JSONObject(body)
                    val registered = json.optBoolean("registered", false)

                    runOnUiThread {
                        layoutRegister.visibility = View.VISIBLE
                        isRegistered = registered
                        if (registered) {
                            tvRegistrationStatus.text = "Phone registered"
                            tvRegistrationStatus.setBackgroundColor(0xFFC8E6C9.toInt())
                        } else {
                            tvRegistrationStatus.text = "Phone not registered"
                            tvRegistrationStatus.setBackgroundColor(0xFFFFCDD2.toInt())
                        }
                        updateRegistrationButtons()
                    }
                } catch (e: Exception) {
                    runOnUiThread {
                        tvRegistrationStatus.text = "Registration check failed"
                        tvRegistrationStatus.setBackgroundColor(0xFFFFCDD2.toInt())
                        layoutRegister.visibility = View.VISIBLE
                        log("Registration check failed: ${e.message}")
                    }
                }
            }.start()
        }
    }

    private fun setupRegistration() {
        btnRegister = findViewById(R.id.btnRegister)
        btnUnregister = findViewById(R.id.btnUnregister)
        btnRegister.setOnClickListener { registerPhone() }
        btnUnregister.setOnClickListener { unregisterPhone() }
        updateRegistrationButtons()
    }

    private fun updateRegistrationButtons() {
        btnRegister.isEnabled = !isRegistered
        btnUnregister.isEnabled = isRegistered
    }

    private fun registerPhone() {
        if (isOpenSourceMode()) {
            log("Registration not needed in open source mode")
            return
        }

        val user = FirebaseAuth.getInstance().currentUser
        if (user == null) {
            log("Not signed in")
            return
        }

        log("Registering phone...")
        tvRegistrationStatus.text = "Registering..."
        tvRegistrationStatus.setBackgroundColor(0xFFFFF9C4.toInt())

        FirebaseMessaging.getInstance().token.addOnSuccessListener { fcmToken ->
            user.getIdToken(false).addOnSuccessListener { result ->
                val idToken = result.token ?: return@addOnSuccessListener
                val apiUrl = getApiUrl()

                Thread {
                    try {
                        val body = JSONObject().apply {
                            put("fcmToken", fcmToken)
                            put("deviceName", android.os.Build.MODEL)
                            put("deviceModel", "${android.os.Build.MANUFACTURER} ${android.os.Build.MODEL}")
                            put("deviceId", getDeviceUUID())
                        }

                        val request = Request.Builder()
                            .url("$apiUrl/api/devices/register")
                            .post(body.toString().toRequestBody("application/json".toMediaType()))
                            .addHeader("Authorization", "Bearer $idToken")
                            .build()

                        val response = httpClient.newCall(request).execute()
                        runOnUiThread {
                            if (response.isSuccessful) {
                                log("Phone registered successfully")
                                // Also set the API URL for FcmService
                                FcmService.apiBaseUrl = apiUrl
                                // Verify registration status from server
                                checkRegistration()
                            } else {
                                log("Registration failed: ${response.code}")
                                tvRegistrationStatus.text = "Registration failed"
                                tvRegistrationStatus.setBackgroundColor(0xFFFFCDD2.toInt())
                            }
                        }
                    } catch (e: Exception) {
                        runOnUiThread {
                            log("Registration error: ${e.message}")
                            tvRegistrationStatus.text = "Registration error"
                            tvRegistrationStatus.setBackgroundColor(0xFFFFCDD2.toInt())
                        }
                    }
                }.start()
            }
        }
    }

    private fun unregisterPhone() {
        if (isOpenSourceMode()) {
            log("Unregister not needed in open source mode")
            return
        }

        val user = FirebaseAuth.getInstance().currentUser
        if (user == null) {
            log("Not signed in")
            return
        }

        log("Unregistering phone...")
        tvRegistrationStatus.text = "Unregistering..."
        tvRegistrationStatus.setBackgroundColor(0xFFFFF9C4.toInt())

        user.getIdToken(false).addOnSuccessListener { result ->
            val idToken = result.token ?: return@addOnSuccessListener
            val apiUrl = getApiUrl()

            Thread {
                try {
                    val body = JSONObject().apply {
                        put("deviceId", getDeviceUUID())
                    }

                    val request = Request.Builder()
                        .url("$apiUrl/api/devices/delete")
                        .post(body.toString().toRequestBody("application/json".toMediaType()))
                        .addHeader("Authorization", "Bearer $idToken")
                        .build()

                    val response = httpClient.newCall(request).execute()
                    runOnUiThread {
                        if (response.isSuccessful) {
                            log("Phone unregistered")
                        } else {
                            log("Unregister response: ${response.code}")
                        }
                        // Always re-check actual registration status from server
                        checkRegistration()
                    }
                } catch (e: Exception) {
                    runOnUiThread {
                        log("Unregister error: ${e.message}")
                    }
                }
            }.start()
        }
    }

    // --- Rest of the UI ---

    private fun setupScreenshotButton() {
        findViewById<Button>(R.id.btnScreenshot).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            log("Taking screenshot...")
            screenshotManager.takeScreenshot(service) { result ->
                runOnUiThread {
                    result.onSuccess { file ->
                        log("Screenshot saved: ${file.name}")
                        val bitmap = BitmapFactory.decodeFile(file.absolutePath)
                        ivScreenshot.setImageBitmap(bitmap)
                    }
                    result.onFailure { e ->
                        log("Screenshot failed: ${e.message}")
                    }
                }
            }
        }
    }

    private fun setupClickButton() {
        val etX = findViewById<EditText>(R.id.etClickX)
        val etY = findViewById<EditText>(R.id.etClickY)
        findViewById<Button>(R.id.btnClick).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            val x = etX.text.toString().toFloatOrNull()
            val y = etY.text.toString().toFloatOrNull()
            if (x == null || y == null) { log("Enter valid X and Y"); return@setOnClickListener }
            log("Clicking at ($x, $y)...")
            service.click(x, y, callback = object : AccessibilityService.GestureResultCallback() {
                override fun onCompleted(g: android.accessibilityservice.GestureDescription?) { runOnUiThread { log("Click completed at ($x, $y)") } }
                override fun onCancelled(g: android.accessibilityservice.GestureDescription?) { runOnUiThread { log("Click cancelled") } }
            })
        }
    }

    private fun setupDragButton() {
        val etSX = findViewById<EditText>(R.id.etDragStartX)
        val etSY = findViewById<EditText>(R.id.etDragStartY)
        val etEX = findViewById<EditText>(R.id.etDragEndX)
        val etEY = findViewById<EditText>(R.id.etDragEndY)
        findViewById<Button>(R.id.btnDrag).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            val sx = etSX.text.toString().toFloatOrNull(); val sy = etSY.text.toString().toFloatOrNull()
            val ex = etEX.text.toString().toFloatOrNull(); val ey = etEY.text.toString().toFloatOrNull()
            if (sx == null || sy == null || ex == null || ey == null) { log("Enter valid coords"); return@setOnClickListener }
            log("Dragging ($sx,$sy) to ($ex,$ey)...")
            service.drag(sx, sy, ex, ey, callback = object : AccessibilityService.GestureResultCallback() {
                override fun onCompleted(g: android.accessibilityservice.GestureDescription?) { runOnUiThread { log("Drag completed") } }
                override fun onCancelled(g: android.accessibilityservice.GestureDescription?) { runOnUiThread { log("Drag cancelled") } }
            })
        }
    }

    private fun setupTypeButton() {
        val etText = findViewById<EditText>(R.id.etTypeText)
        findViewById<Button>(R.id.btnType).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            val text = etText.text.toString()
            if (text.isEmpty()) { log("Enter text"); return@setOnClickListener }
            etText.clearFocus()
            log("Typing in 3s...")
            handler.postDelayed({
                val ok = service.typeText(text)
                runOnUiThread { log(if (ok) "Typed: $text" else "Type failed") }
            }, 3000)
        }
        findViewById<Button>(R.id.btnGetText).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            log("Getting text in 3s...")
            handler.postDelayed({
                val text = service.getTextFromFocused()
                runOnUiThread { log(if (text != null) "Text: $text" else "Get text failed") }
            }, 3000)
        }
    }

    private fun setupClipboardButtons() {
        findViewById<Button>(R.id.btnSelectAll).setOnClickListener { val s = requireService() ?: return@setOnClickListener; log(if (s.selectAll()) "Select All done" else "Select All failed") }
        findViewById<Button>(R.id.btnCopy).setOnClickListener { val s = requireService() ?: return@setOnClickListener; log(if (s.copy()) "Copy done" else "Copy failed") }
        findViewById<Button>(R.id.btnPaste).setOnClickListener { val s = requireService() ?: return@setOnClickListener; log(if (s.paste()) "Paste done" else "Paste failed") }
    }

    private fun setupGlobalActionButtons() {
        findViewById<Button>(R.id.btnBack).setOnClickListener { requireService()?.pressBack(); log("Back") }
        findViewById<Button>(R.id.btnHome).setOnClickListener { requireService()?.pressHome(); log("Home") }
        findViewById<Button>(R.id.btnRecents).setOnClickListener { requireService()?.pressRecents(); log("Recents") }
    }

    private fun setupCameraButtons() {
        val ivCamera = findViewById<ImageView>(R.id.ivCamera)

        findViewById<Button>(R.id.btnListCameras).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            val cameraManager = service.getSystemService(android.content.Context.CAMERA_SERVICE) as android.hardware.camera2.CameraManager
            try {
                val ids = cameraManager.cameraIdList
                val info = ids.map { id ->
                    val chars = cameraManager.getCameraCharacteristics(id)
                    val facing = when (chars.get(android.hardware.camera2.CameraCharacteristics.LENS_FACING)) {
                        android.hardware.camera2.CameraCharacteristics.LENS_FACING_BACK -> "back"
                        android.hardware.camera2.CameraCharacteristics.LENS_FACING_FRONT -> "front"
                        android.hardware.camera2.CameraCharacteristics.LENS_FACING_EXTERNAL -> "external"
                        else -> "unknown"
                    }
                    "$id ($facing)"
                }
                log("Cameras: ${info.joinToString(", ")}")
            } catch (e: Exception) {
                log("List cameras failed: ${e.message}")
            }
        }

        findViewById<Button>(R.id.btnTakePhoto).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            log("Capturing camera 0...")
            service.captureCamera("0") { bitmap ->
                runOnUiThread {
                    if (bitmap != null) {
                        ivCamera.setImageBitmap(bitmap)
                        log("Camera capture OK (${bitmap.width}x${bitmap.height})")
                    } else {
                        log("Camera capture failed")
                    }
                }
            }
        }
    }

    private fun setupUiTreeButton() {
        val tvUiTree = findViewById<TextView>(R.id.tvUiTree)
        findViewById<Button>(R.id.btnGetUiTree).setOnClickListener {
            val service = requireService() ?: return@setOnClickListener
            log("Getting UI tree in 3s...")
            handler.postDelayed({
                val tree = service.getUiTree()
                runOnUiThread { tvUiTree.text = tree; log("UI tree (${tree.lines().size} nodes)") }
            }, 3000)
        }
    }

    private fun log(message: String) {
        val time = SimpleDateFormat("HH:mm:ss.SSS", Locale.getDefault()).format(Date())
        val current = tvLog.text.toString()
        val lines = current.split("\n").takeLast(30)
        tvLog.text = (lines + "[$time] $message").joinToString("\n")
    }
}
