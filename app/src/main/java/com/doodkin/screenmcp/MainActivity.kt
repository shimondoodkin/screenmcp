package com.doodkin.screenmcp

import android.accessibilityservice.AccessibilityService
import android.content.Intent
import android.graphics.BitmapFactory
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

class MainActivity : AppCompatActivity() {

    private lateinit var tvStatus: TextView
    private lateinit var tvLog: TextView
    private lateinit var tvConnectionStatus: TextView
    private lateinit var tvRegistrationStatus: TextView
    private lateinit var layoutRegister: LinearLayout
    private lateinit var ivScreenshot: ImageView
    private lateinit var screenshotManager: ScreenshotManager
    private lateinit var etApiUrl: EditText

    private val handler = Handler(Looper.getMainLooper())
    private val httpClient = OkHttpClient()
    private val statusChecker = object : Runnable {
        override fun run() {
            updateServiceStatus()
            updateConnectionStatus()
            handler.postDelayed(this, 1000)
        }
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
        etApiUrl = findViewById(R.id.etApiUrl)

        setupUserInfo()
        setupAccessibilityButton()
        setupRegistration()
        setupConnectionControls()
        setupScreenshotButton()
        setupClickButton()
        setupDragButton()
        setupTypeButton()
        setupClipboardButtons()
        setupGlobalActionButtons()
        setupUiTreeButton()

        // Check registration on load
        checkRegistration()
    }

    override fun onResume() {
        super.onResume()
        handler.post(statusChecker)
    }

    override fun onPause() {
        super.onPause()
        handler.removeCallbacks(statusChecker)
    }

    private fun getApiUrl(): String {
        val custom = etApiUrl.text.toString().trim()
        return custom.ifEmpty { "https://server10.doodkin.com" }
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
            tvConnectionStatus.text = "Worker: ${svc.getStatus()}"
            if (svc.isConnected()) {
                tvConnectionStatus.setBackgroundColor(0xFFC8E6C9.toInt())
            } else {
                tvConnectionStatus.setBackgroundColor(0xFFFFF9C4.toInt())
            }
        } else {
            tvConnectionStatus.text = "Worker: Not started"
            tvConnectionStatus.setBackgroundColor(0xFFFFCDD2.toInt())
        }
    }

    private fun setupUserInfo() {
        val tvUser = findViewById<TextView>(R.id.tvUser)
        val user = FirebaseAuth.getInstance().currentUser
        tvUser.text = user?.email ?: user?.displayName ?: "Not signed in"

        findViewById<Button>(R.id.btnSignOut).setOnClickListener {
            ConnectionService.instance?.disconnect()
            FirebaseAuth.getInstance().signOut()
            startActivity(Intent(this, LoginActivity::class.java))
            finish()
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

    // --- Registration ---

    private fun checkRegistration() {
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
                        if (registered) {
                            tvRegistrationStatus.text = "Phone registered"
                            tvRegistrationStatus.setBackgroundColor(0xFFC8E6C9.toInt())
                            layoutRegister.visibility = View.GONE
                        } else {
                            tvRegistrationStatus.text = "Phone not registered"
                            tvRegistrationStatus.setBackgroundColor(0xFFFFCDD2.toInt())
                            layoutRegister.visibility = View.VISIBLE
                        }
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
        findViewById<Button>(R.id.btnRegister).setOnClickListener {
            registerPhone()
        }
    }

    private fun registerPhone() {
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
                                tvRegistrationStatus.text = "Phone registered"
                                tvRegistrationStatus.setBackgroundColor(0xFFC8E6C9.toInt())
                                layoutRegister.visibility = View.GONE

                                // Also set the API URL for FcmService
                                FcmService.apiBaseUrl = apiUrl
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

    // --- Connection ---

    private fun setupConnectionControls() {
        val btnConnect = findViewById<Button>(R.id.btnConnect)
        val btnDisconnect = findViewById<Button>(R.id.btnDisconnect)

        btnConnect.setOnClickListener {
            val user = FirebaseAuth.getInstance().currentUser
            if (user == null) {
                log("Not signed in")
                return@setOnClickListener
            }

            val apiUrl = getApiUrl()
            log("Getting auth token...")

            user.getIdToken(false).addOnSuccessListener { result ->
                val token = result.token
                if (token == null) {
                    log("Failed to get token")
                    return@addOnSuccessListener
                }

                log("Discovering worker via $apiUrl...")
                val intent = Intent(this, ConnectionService::class.java).apply {
                    putExtra(ConnectionService.EXTRA_API_URL, apiUrl)
                    putExtra(ConnectionService.EXTRA_TOKEN, token)
                }
                startForegroundService(intent)
            }
        }

        btnDisconnect.setOnClickListener {
            ConnectionService.instance?.disconnect()
            log("Disconnected from worker")
        }
    }

    // --- Rest of the UI (unchanged) ---

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
        val time = SimpleDateFormat("HH:mm:ss", Locale.getDefault()).format(Date())
        val current = tvLog.text.toString()
        val lines = current.split("\n").takeLast(20)
        tvLog.text = (lines + "[$time] $message").joinToString("\n")
    }
}
