# Android File Logging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in file logger to the Android client that persists `AppLog` output to a rotating 3-file × 10 MB log set, plus diagnostic context around connect/disconnect, so reconnection "blink" cycles can be debugged after the fact.

**Architecture:** New singleton `FileLogger` owns disk I/O on its own `HandlerThread`. The single existing chokepoint `AppLog.add()` calls into it — every existing `WS`/`SSE`/`Conn`/`UI` tag is captured with no per-call-site change. UI checkbox in `MainActivity` toggles a SharedPreferences flag. A handful of targeted `tlog` calls in `WebSocketClient`, `ConnectionService`, `SseService`, and `ScreenMcpService` add the diagnostic context.

**Tech Stack:** Kotlin, Android SDK, `okhttp3` (already used), `androidx.core` `FileProvider` (already on classpath), `BufferedWriter`+`FileOutputStream` for disk I/O. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-05-12-android-file-logging-design.md`

---

## Build & Test Commands (reference)

```bash
cd android && ./gradlew assembleDebug                                # build
adb install -r android/app/build/outputs/apk/debug/app-debug.apk     # install
adb logcat -s WebSocketClient ScreenMcpService ConnectionService SseService FileLogger  # tail
adb pull /sdcard/Android/data/com.doodkin.screenmcp/files/logs/      # pull logs
```

This codebase has no JVM unit tests for the Android module. Verification is via build + on-device check. Each task lists exactly what to verify on the device.

---

## File Structure

**New files:**
- `android/app/src/main/java/com/doodkin/screenmcp/FileLogger.kt` — singleton with HandlerThread, rotation, prefs.
- `android/app/src/main/res/xml/file_paths.xml` — FileProvider config.

**Modified files (in order touched by tasks):**
- `android/app/src/main/java/com/doodkin/screenmcp/AppLog.kt` — one new call.
- `android/app/src/main/AndroidManifest.xml` — `<provider>` entry.
- `android/app/src/main/res/layout/activity_main.xml` — checkbox + 2 buttons + path TextView at bottom.
- `android/app/src/main/java/com/doodkin/screenmcp/MainActivity.kt` — init + UI wiring.
- `android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt` — gen prefix, ping/pong, close-code names, lastAuthOkMs.
- `android/app/src/main/java/com/doodkin/screenmcp/ConnectionService.kt` — skipCount + lifecycle logs.
- `android/app/src/main/java/com/doodkin/screenmcp/SseService.kt` — event gap + "SSE dropped while WS up" warning.
- `android/app/src/main/java/com/doodkin/screenmcp/ScreenMcpService.kt` — lifecycle logs via `AppLog`.

---

## Task 1: Create `FileLogger` singleton

**Files:**
- Create: `android/app/src/main/java/com/doodkin/screenmcp/FileLogger.kt`

This object owns all file I/O. It is callable from anywhere (it gracefully no-ops before init or when disabled). When enabled, it dispatches each line to its own `HandlerThread` so callers never block on disk.

- [ ] **Step 1: Write the full `FileLogger.kt`**

```kotlin
package com.doodkin.screenmcp

import android.content.Context
import android.os.Handler
import android.os.HandlerThread
import android.util.Log
import java.io.BufferedWriter
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStreamWriter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Optional file logger. When enabled (via checkbox in MainActivity), every
 * AppLog.add() call is mirrored to a rotating set of files under
 * <external-files>/logs/. Three files × 10 MB = 30 MB worst case.
 *
 * Safe to call before init() and when disabled — both are no-ops.
 */
object FileLogger {
    private const val TAG = "FileLogger"
    private const val MAX_BYTES = 10L * 1024 * 1024  // 10 MB
    private const val MAX_FILES = 3
    private const val PREF_KEY = "file_logging_enabled"

    @Volatile private var enabled = false
    @Volatile private var disabledByError = false
    @Volatile private var logDir: File? = null

    private var thread: HandlerThread? = null
    private var handler: Handler? = null
    private var writer: BufferedWriter? = null
    private var currentSize: Long = 0L

    private val tsFormat = SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US)

    /** Call once from MainActivity.onCreate. Idempotent. */
    fun init(context: Context) {
        val ext = context.getExternalFilesDir(null)
        if (ext == null) {
            Log.w(TAG, "No external files dir; file logging disabled")
            return
        }
        logDir = File(ext, "logs").also { it.mkdirs() }
        val prefs = context.getSharedPreferences("screenmcp", Context.MODE_PRIVATE)
        val want = prefs.getBoolean(PREF_KEY, false)
        if (want) setEnabled(context, true)
    }

    fun isEnabled(): Boolean = enabled && !disabledByError

    fun getLogDir(): File? = logDir

    fun getLogDirPath(): String = logDir?.absolutePath ?: "(unavailable)"

    fun setEnabled(context: Context, on: Boolean) {
        val prefs = context.getSharedPreferences("screenmcp", Context.MODE_PRIVATE)
        prefs.edit().putBoolean(PREF_KEY, on).apply()
        if (on) {
            disabledByError = false
            startThread()
            enabled = true
        } else {
            enabled = false
            stopThread()
        }
    }

    fun clear() {
        post {
            try {
                closeWriter()
                val dir = logDir ?: return@post
                for (i in 0 until MAX_FILES) {
                    val f = if (i == 0) File(dir, "screenmcp.log") else File(dir, "screenmcp.$i.log")
                    if (f.exists()) f.delete()
                }
                currentSize = 0L
            } catch (e: Exception) {
                Log.w(TAG, "clear failed: ${e.message}")
            }
        }
    }

    /** Cheap when disabled: one volatile read. */
    fun log(tag: String, msg: String) {
        if (!enabled || disabledByError) return
        val now = System.currentTimeMillis()
        post {
            try {
                ensureWriter()
                val w = writer ?: return@post
                val line = "${tsFormat.format(Date(now))} [$tag] $msg\n"
                val bytes = line.toByteArray(Charsets.UTF_8).size
                if (currentSize + bytes > MAX_BYTES) {
                    rotate()
                    ensureWriter()
                }
                writer?.write(line)
                writer?.flush()
                currentSize += bytes
            } catch (e: Exception) {
                Log.w(TAG, "write failed, disabling: ${e.message}")
                disabledByError = true
                closeWriter()
            }
        }
    }

    // --- internals ---

    private fun startThread() {
        if (thread != null) return
        thread = HandlerThread("FileLogger").also { it.start() }
        handler = Handler(thread!!.looper)
    }

    private fun stopThread() {
        post { closeWriter() }
        handler?.post { thread?.quitSafely() }
        handler = null
        thread = null
    }

    private fun post(r: () -> Unit) {
        val h = handler ?: return
        h.post(r)
    }

    private fun ensureWriter() {
        if (writer != null) return
        val dir = logDir ?: return
        dir.mkdirs()
        val current = File(dir, "screenmcp.log")
        currentSize = if (current.exists()) current.length() else 0L
        writer = BufferedWriter(OutputStreamWriter(FileOutputStream(current, true), Charsets.UTF_8))
    }

    private fun closeWriter() {
        try { writer?.flush(); writer?.close() } catch (_: Exception) {}
        writer = null
    }

    private fun rotate() {
        closeWriter()
        val dir = logDir ?: return
        // Delete the oldest (screenmcp.{MAX_FILES-1}.log)
        File(dir, "screenmcp.${MAX_FILES - 1}.log").delete()
        // Shift down: screenmcp.{N-1}.log -> screenmcp.N.log
        for (i in MAX_FILES - 1 downTo 2) {
            val src = File(dir, "screenmcp.${i - 1}.log")
            val dst = File(dir, "screenmcp.$i.log")
            if (src.exists()) src.renameTo(dst)
        }
        // Current -> screenmcp.1.log
        val current = File(dir, "screenmcp.log")
        if (current.exists()) current.renameTo(File(dir, "screenmcp.1.log"))
        currentSize = 0L
        // Write a rotation marker
        try {
            val marker = "${tsFormat.format(Date())} [FileLogger] --- rotated ---\n"
            ensureWriter()
            writer?.write(marker)
            writer?.flush()
            currentSize += marker.toByteArray(Charsets.UTF_8).size
        } catch (_: Exception) {}
    }
}
```

- [ ] **Step 2: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL. `FileLogger` compiles but is not yet wired in.

- [ ] **Step 3: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/java/com/doodkin/screenmcp/FileLogger.kt
git -C /home/user/screenmcp commit -m "feat(android): add FileLogger singleton with rotation"
```

---

## Task 2: Wire `FileLogger.log` into `AppLog.add`

**Files:**
- Modify: `android/app/src/main/java/com/doodkin/screenmcp/AppLog.kt`

After this task, every existing log entry funnels into `FileLogger`. It's still a no-op until the checkbox enables it.

- [ ] **Step 1: Edit `AppLog.kt` to add the FileLogger call**

Replace the entire body of `add()`:

```kotlin
fun add(tag: String, msg: String) {
    val time = SimpleDateFormat("HH:mm:ss.SSS", Locale.getDefault()).format(Date())
    synchronized(entries) {
        entries.add("[$time][$tag] $msg")
        if (entries.size > 200) entries.removeAt(0)
        version++
    }
    FileLogger.log(tag, msg)
}
```

(Only one line added: `FileLogger.log(tag, msg)` outside the `synchronized` block — `FileLogger.log` already enqueues to its own thread.)

- [ ] **Step 2: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 3: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/java/com/doodkin/screenmcp/AppLog.kt
git -C /home/user/screenmcp commit -m "feat(android): mirror AppLog to FileLogger"
```

---

## Task 3: Add `FileProvider` configuration

**Files:**
- Create: `android/app/src/main/res/xml/file_paths.xml`
- Modify: `android/app/src/main/AndroidManifest.xml`

The "Open Logs Folder" button uses a `content://` URI from `androidx.core.content.FileProvider`. The provider needs a `<provider>` entry and a paths resource.

- [ ] **Step 1: Create `file_paths.xml`**

```xml
<?xml version="1.0" encoding="utf-8"?>
<paths xmlns:android="http://schemas.android.com/apk/res/android">
    <external-files-path name="logs" path="logs/" />
</paths>
```

- [ ] **Step 2: Add the `<provider>` block to `AndroidManifest.xml`**

Insert immediately before the closing `</application>` tag:

```xml
        <provider
            android:name="androidx.core.content.FileProvider"
            android:authorities="${applicationId}.fileprovider"
            android:exported="false"
            android:grantUriPermissions="true">
            <meta-data
                android:name="android.support.FILE_PROVIDER_PATHS"
                android:resource="@xml/file_paths" />
        </provider>
```

- [ ] **Step 3: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 4: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/res/xml/file_paths.xml android/app/src/main/AndroidManifest.xml
git -C /home/user/screenmcp commit -m "feat(android): register FileProvider for log folder access"
```

---

## Task 4: Add UI checkbox + buttons to `activity_main.xml`

**Files:**
- Modify: `android/app/src/main/res/layout/activity_main.xml`

Insert a debug-log section immediately above the existing `tvLog` TextView at the bottom of the main `LinearLayout` (inside the `ScrollView`).

- [ ] **Step 1: Insert XML block above `tvLog`**

Find this existing block:

```xml
        <!-- Log Output -->
        <TextView
            android:id="@+id/tvLog"
```

Insert immediately before it:

```xml
        <!-- Debug File Logging -->
        <View
            android:layout_width="match_parent"
            android:layout_height="1dp"
            android:background="#CCCCCC"
            android:layout_marginTop="8dp"
            android:layout_marginBottom="8dp" />

        <CheckBox
            android:id="@+id/cbFileLogging"
            android:layout_width="match_parent"
            android:layout_height="wrap_content"
            android:text="Write debug log to file" />

        <TextView
            android:id="@+id/tvLogPath"
            android:layout_width="match_parent"
            android:layout_height="wrap_content"
            android:textSize="10sp"
            android:fontFamily="monospace"
            android:textColor="#666666"
            android:visibility="gone"
            android:layout_marginBottom="4dp" />

        <LinearLayout
            android:layout_width="match_parent"
            android:layout_height="wrap_content"
            android:orientation="horizontal"
            android:layout_marginBottom="8dp">

            <Button
                android:id="@+id/btnOpenLogsFolder"
                android:layout_width="0dp"
                android:layout_height="wrap_content"
                android:layout_weight="1"
                android:text="Open Logs Folder"
                android:layout_marginEnd="4dp" />

            <Button
                android:id="@+id/btnClearLogs"
                android:layout_width="0dp"
                android:layout_height="wrap_content"
                android:layout_weight="1"
                android:text="Clear Logs" />
        </LinearLayout>
```

- [ ] **Step 2: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL (resource ids `cbFileLogging`, `tvLogPath`, `btnOpenLogsFolder`, `btnClearLogs` are generated).

- [ ] **Step 3: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/res/layout/activity_main.xml
git -C /home/user/screenmcp commit -m "feat(android): add debug-log UI controls to main layout"
```

---

## Task 5: Wire UI controls in `MainActivity`

**Files:**
- Modify: `android/app/src/main/java/com/doodkin/screenmcp/MainActivity.kt`

Add a `setupFileLogging()` helper, call it from `onCreate`, and init `FileLogger` early.

- [ ] **Step 1: Add `CheckBox` import**

At the top of `MainActivity.kt`, add to the existing `android.widget.*` imports:

```kotlin
import android.widget.CheckBox
import android.widget.Toast
import androidx.core.content.FileProvider
```

(`Button`, `EditText`, `ImageView`, `LinearLayout`, `TextView` are already imported.)

- [ ] **Step 2: Init `FileLogger` in `onCreate`**

In `MainActivity.onCreate`, immediately after `super.onCreate(savedInstanceState)` and `setContentView(R.layout.activity_main)`, add:

```kotlin
        FileLogger.init(this)
```

(Place before the existing `screenshotManager = ScreenshotManager(cacheDir)` line.)

- [ ] **Step 3: Add `setupFileLogging` call**

In the list of `setupXxx()` calls inside `onCreate`, add at the end (after `setupUiTreeButton()`):

```kotlin
        setupFileLogging()
```

- [ ] **Step 4: Add the `setupFileLogging` method**

Insert this method just before the existing `private fun log(message: String)` at the bottom of the class:

```kotlin
    private fun setupFileLogging() {
        val cb = findViewById<CheckBox>(R.id.cbFileLogging)
        val tvPath = findViewById<TextView>(R.id.tvLogPath)
        val btnOpen = findViewById<Button>(R.id.btnOpenLogsFolder)
        val btnClear = findViewById<Button>(R.id.btnClearLogs)

        fun refreshPath() {
            val on = FileLogger.isEnabled()
            tvPath.visibility = if (on) View.VISIBLE else View.GONE
            tvPath.text = "Path: " + FileLogger.getLogDirPath()
        }

        cb.isChecked = FileLogger.isEnabled()
        refreshPath()

        cb.setOnCheckedChangeListener { _, checked ->
            FileLogger.setEnabled(this, checked)
            refreshPath()
            log(if (checked) "File logging enabled" else "File logging disabled")
        }

        btnOpenLogsFolder@ btnOpen.setOnClickListener {
            val dir = FileLogger.getLogDir()
            if (dir == null) {
                Toast.makeText(this, "Logs folder unavailable", Toast.LENGTH_SHORT).show()
                return@setOnClickListener
            }
            try {
                val uri = FileProvider.getUriForFile(
                    this,
                    "$packageName.fileprovider",
                    dir
                )
                val intent = Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, "resource/folder")
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                if (intent.resolveActivity(packageManager) != null) {
                    startActivity(intent)
                } else {
                    Toast.makeText(this, "Logs at: ${dir.absolutePath}", Toast.LENGTH_LONG).show()
                }
            } catch (e: Exception) {
                Toast.makeText(this, "Logs at: ${dir.absolutePath}", Toast.LENGTH_LONG).show()
            }
        }

        btnClear.setOnClickListener {
            FileLogger.clear()
            Toast.makeText(this, "Log files cleared", Toast.LENGTH_SHORT).show()
        }
    }
```

- [ ] **Step 5: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 6: Install and smoke-test**

```bash
adb install -r /home/user/screenmcp/android/app/build/outputs/apk/debug/app-debug.apk
```

Open the app, scroll to the bottom. Verify:
- A checkbox labeled "Write debug log to file" appears.
- Two buttons "Open Logs Folder" / "Clear Logs" appear below it.
- Checking the box shows a path TextView like `/storage/emulated/0/Android/data/com.doodkin.screenmcp/files/logs`.
- The on-screen log shows `[UI] File logging enabled`.
- After ~5 seconds, pull and inspect:

```bash
adb shell ls -la /sdcard/Android/data/com.doodkin.screenmcp/files/logs/
adb pull /sdcard/Android/data/com.doodkin.screenmcp/files/logs/screenmcp.log /tmp/screenmcp.log
head /tmp/screenmcp.log
```

Expected: `screenmcp.log` exists, contains lines like `2026-05-12 09:21:14.483 [UI] File logging enabled`.

- [ ] **Step 7: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/java/com/doodkin/screenmcp/MainActivity.kt
git -C /home/user/screenmcp commit -m "feat(android): wire file-logging checkbox and buttons in MainActivity"
```

---

## Task 6: Diagnostic context in `WebSocketClient`

**Files:**
- Modify: `android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt`

Add the `[gen=N]` prefix to every `tlog`, log ping/pong arrival, record last `auth_ok` timestamp, and decode close codes.

- [ ] **Step 1: Add a `lastAuthOkMs` field**

In the field block near the top of the class (right after `@Volatile private var connectionGeneration = 0L`), add:

```kotlin
    /** Wall-clock ms of the most recent successful auth_ok, 0 if never */
    @Volatile private var lastAuthOkMs = 0L
```

- [ ] **Step 2: Update `tlog` to include generation prefix**

Replace the existing `tlog` method:

```kotlin
    private fun tlog(msg: String) {
        val ts = SimpleDateFormat("HH:mm:ss.SSS", Locale.US).format(Date())
        val gen = connectionGeneration
        Log.i(TAG, msg)
        onLog?.invoke("[$ts] [gen=$gen] $msg")
    }
```

- [ ] **Step 3: Add close-code decoder**

Add this private method anywhere inside the class (e.g. just below `tlog`):

```kotlin
    private fun closeCodeName(code: Int): String = when (code) {
        1000 -> "normal"
        1001 -> "going_away"
        1002 -> "protocol_error"
        1003 -> "unsupported_data"
        1006 -> "abnormal"
        1008 -> "policy_violation"
        1011 -> "server_error"
        in 4000..4999 -> "app_$code"
        else -> "code_$code"
    }
```

- [ ] **Step 4: Record `lastAuthOkMs` and log ping/pong**

In `handleMessage`, find the `"auth_ok" ->` branch and add the timestamp set right before `isConnected.set(true)`:

```kotlin
                "auth_ok" -> {
                    val totalMs = if (connectStartMs > 0) System.currentTimeMillis() - connectStartMs else 0
                    tlog("Authenticated (total connect: ${totalMs}ms)")
                    lastAuthOkMs = System.currentTimeMillis()
                    isConnected.set(true)
                    isConnecting.set(false)
                    reconnectAttempt = 0
                    handler.post { onStatusChange("Connected") }
                }
```

And replace the silent `"ping" ->` branch:

```kotlin
                "ping" -> {
                    tlog("ping received, sending pong")
                    ws.send(JSONObject().put("type", "pong").toString())
                }
```

- [ ] **Step 5: Enrich `onClosed` and `onFailure`**

In `doConnect`'s `WebSocketListener`, replace `onClosed` and `onFailure`:

```kotlin
            override fun onClosed(ws: WebSocket, code: Int, reason: String) {
                val ageMs = if (lastAuthOkMs > 0) System.currentTimeMillis() - lastAuthOkMs else -1L
                tlog("WS closed: $code (${closeCodeName(code)}) reason='$reason' ageSinceAuth=${ageMs}ms")
                isConnected.set(false)
                isConnecting.set(false)
                if (myGeneration != connectionGeneration) return
                handler.post { onStatusChange("Disconnected") }
                scheduleReconnect()
            }

            override fun onFailure(ws: WebSocket, t: Throwable, response: Response?) {
                val ageMs = if (lastAuthOkMs > 0) System.currentTimeMillis() - lastAuthOkMs else -1L
                tlog("WS failure: ${t.message} httpCode=${response?.code} ageSinceAuth=${ageMs}ms")
                isConnected.set(false)
                isConnecting.set(false)
                if (myGeneration != connectionGeneration) return
                handler.post { onStatusChange("Connection failed: ${t.message}") }
                scheduleReconnect()
            }
```

- [ ] **Step 6: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 7: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt
git -C /home/user/screenmcp commit -m "feat(android): log gen/ping-pong/close-code/age in WebSocketClient"
```

---

## Task 7: Diagnostic context in `ConnectionService`

**Files:**
- Modify: `android/app/src/main/java/com/doodkin/screenmcp/ConnectionService.kt`

Track how many SSE-triggered "already connected, skip" events fire between real connects, and log service lifecycle.

- [ ] **Step 1: Add a skip counter field**

Inside the `ConnectionService` class, near `currentStatus`, add:

```kotlin
    private var skipCount = 0
```

- [ ] **Step 2: Log `onCreate` and `onDestroy`**

Replace the existing `onCreate`:

```kotlin
    override fun onCreate() {
        super.onCreate()
        instance = this
        createNotificationChannel()
        AppLog.add("Conn", "ConnectionService onCreate")
    }
```

Replace the existing `onDestroy`:

```kotlin
    override fun onDestroy() {
        AppLog.add("Conn", "ConnectionService onDestroy")
        ScreenMcpService.instance?.onConnectionStatusChange = null
        ScreenMcpService.instance?.onLog = null
        instance = null
        super.onDestroy()
    }
```

- [ ] **Step 3: Track skip count in `onStartCommand`**

Inside `onStartCommand`, replace the existing "skip if already connected" branch and the "service.disconnectWorker(); connectDirect/connectViaApi" branch so the counter increments on skip and resets on real connect:

```kotlin
        if (token != null) {
            // Skip if already connected to the same worker
            if (wsUrl != null && service.isWorkerConnectedTo(wsUrl)) {
                skipCount++
                Log.i(TAG, "Already connected to $wsUrl, skipping (skip #$skipCount since last connect)")
                AppLog.add("Conn", "Already connected to $wsUrl, skipping (skip #$skipCount since last connect)")
                return START_STICKY
            }

            skipCount = 0
            service.disconnectWorker()

            if (wsUrl != null) {
                Log.i(TAG, "Direct connect to $wsUrl (fallback API: $apiUrl)")
                AppLog.add("Conn", "Connecting → $wsUrl")
                service.connectDirect(wsUrl, token, fallbackApiUrl = apiUrl, deviceId = deviceId)
            } else if (apiUrl != null) {
                Log.i(TAG, "Discover via $apiUrl")
                AppLog.add("Conn", "Discovering via $apiUrl")
                service.connectViaApi(apiUrl, token, deviceId = deviceId)
            }
        }
```

- [ ] **Step 4: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 5: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/java/com/doodkin/screenmcp/ConnectionService.kt
git -C /home/user/screenmcp commit -m "feat(android): log lifecycle + skip-count in ConnectionService"
```

---

## Task 8: Diagnostic context in `SseService`

**Files:**
- Modify: `android/app/src/main/java/com/doodkin/screenmcp/SseService.kt`

Log the gap between SSE events and flag the "SSE dropped while WS was still up" case.

- [ ] **Step 1: Add `lastEventMs` field**

In the field block near `private var shouldReconnect = true`, add:

```kotlin
    @Volatile private var lastEventMs = 0L
```

- [ ] **Step 2: Annotate `onEvent` with gap**

Inside `connectSseToUrl`'s `EventSourceListener`, replace the `onEvent` override:

```kotlin
            override fun onEvent(eventSource: EventSource, id: String?, type: String?, data: String) {
                val now = System.currentTimeMillis()
                val gapMs = if (lastEventMs > 0) now - lastEventMs else -1L
                lastEventMs = now
                Log.i(TAG, "SSE event: type=$type data=$data")
                val gapStr = if (gapMs >= 0) " (gap ${gapMs}ms since last event)" else ""
                AppLog.add("SSE", "Event: type=$type$gapStr")
                handleSseEvent(data)
            }
```

- [ ] **Step 3: Add "SSE dropped while WS up" warning in `onClosed` and `onFailure`**

Replace the existing `onClosed`:

```kotlin
            override fun onClosed(eventSource: EventSource) {
                val wsUp = ScreenMcpService.instance?.isWorkerConnected() == true
                Log.i(TAG, "SSE closed (wsUp=$wsUp)")
                AppLog.add("SSE", if (wsUp) "Closed (SSE dropped while WS still up)" else "Closed")
                handler.post { updateNotification("SSE disconnected") }
                scheduleReconnect()
            }
```

Replace the existing `onFailure`:

```kotlin
            override fun onFailure(eventSource: EventSource, t: Throwable?, response: Response?) {
                val wsUp = ScreenMcpService.instance?.isWorkerConnected() == true
                Log.e(TAG, "SSE failure: ${t?.message}, response=${response?.code}, wsUp=$wsUp")
                val tail = if (wsUp) " (SSE dropped while WS still up)" else ""
                AppLog.add("SSE", "Failed: ${t?.message}, HTTP ${response?.code}$tail")
                handler.post { updateNotification("SSE connection failed") }
                scheduleReconnect()
            }
```

- [ ] **Step 4: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 5: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/java/com/doodkin/screenmcp/SseService.kt
git -C /home/user/screenmcp commit -m "feat(android): log SSE event gaps and 'dropped while WS up' state"
```

---

## Task 9: Lifecycle logs in `ScreenMcpService`

**Files:**
- Modify: `android/app/src/main/java/com/doodkin/screenmcp/ScreenMcpService.kt`

The accessibility service restarting kills the WS — log it.

- [ ] **Step 1: Add `AppLog` calls to lifecycle methods**

Replace `onServiceConnected`:

```kotlin
    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
        Log.i(TAG, "Accessibility service connected")
        AppLog.add("Svc", "AccessibilityService onServiceConnected")
    }
```

Replace `onDestroy`:

```kotlin
    override fun onDestroy() {
        AppLog.add("Svc", "AccessibilityService onDestroy")
        super.onDestroy()
        disconnectWorker()
        instance = null
        Log.i(TAG, "Accessibility service destroyed")
    }
```

- [ ] **Step 2: Build**

Run: `cd /home/user/screenmcp/android && ./gradlew assembleDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 3: Commit**

```bash
git -C /home/user/screenmcp add android/app/src/main/java/com/doodkin/screenmcp/ScreenMcpService.kt
git -C /home/user/screenmcp commit -m "feat(android): log AccessibilityService lifecycle"
```

---

## Task 10: End-to-end verification

**Files:** none modified.

- [ ] **Step 1: Install and enable logging**

```bash
adb install -r /home/user/screenmcp/android/app/build/outputs/apk/debug/app-debug.apk
```

Open app, check the "Write debug log to file" box. Confirm the path TextView appears.

- [ ] **Step 2: Trigger a real connect cycle**

In the app, sign out + sign in (or toggle the connection mode). Wait ~30 seconds for at least one SSE event and one WS connect.

- [ ] **Step 3: Pull the log file and verify content**

```bash
adb pull /sdcard/Android/data/com.doodkin.screenmcp/files/logs/screenmcp.log /tmp/screenmcp.log
grep -E "Event:|Conn|gen=|auth_ok|WS closed|WS opened|skip" /tmp/screenmcp.log
```

Expected: log lines show a coherent timeline like
```
... [SSE] Event: type=connect (gap 0ms since last event)
... [Conn] ConnectionService onCreate
... [Conn] Connecting → wss://...
... [WS] [gen=1] WS connecting to wss://...
... [WS] [gen=1] WS opened in 134ms, sending auth
... [WS] [gen=1] Authenticated (total connect: 248ms)
... [WS] [gen=1] ping received, sending pong
```

If a blink happens during the test, you should see `WS closed: <code> (<name>) reason='...' ageSinceAuth=<ms>` followed by the next cycle.

- [ ] **Step 4: Verify rotation limit (smoke check)**

Confirm at most 3 files exist in `/sdcard/Android/data/com.doodkin.screenmcp/files/logs/`:

```bash
adb shell ls /sdcard/Android/data/com.doodkin.screenmcp/files/logs/
```

Expected: `screenmcp.log` always; `screenmcp.1.log` and `screenmcp.2.log` only after rotations. No file > 10 MB.

- [ ] **Step 5: Verify "Clear Logs" works**

Tap "Clear Logs" in the UI. Pull again:

```bash
adb shell ls /sdcard/Android/data/com.doodkin.screenmcp/files/logs/
```

Expected: empty or just a freshly opened `screenmcp.log` if logging is still on.

- [ ] **Step 6: Verify disabled-path is silent**

Uncheck the box, wait 10 seconds, pull `screenmcp.log`, confirm no new lines are appended after the "File logging disabled" line.

If all steps pass, the feature is done.

---

## Self-Review Notes

- **Spec coverage:** All sections of the spec map to tasks: `FileLogger` (Task 1) → rotation/init/log/clear API. `AppLog` hook (Task 2). `FileProvider` (Task 3). UI block (Task 4). MainActivity wiring (Task 5). Diagnostic-context additions: WebSocketClient gen+ping+close-code+lastAuthOkMs (Task 6), ConnectionService skipCount+lifecycle (Task 7), SseService gap+wsUp warning (Task 8), ScreenMcpService lifecycle (Task 9). Verification (Task 10).
- **Placeholders:** none.
- **Type consistency:** `FileLogger.log(tag, msg)` matches its call site in `AppLog.add`. `FileLogger.getLogDir()` returns `File?`; `MainActivity` null-checks it. `FileLogger.getLogDirPath()` returns `String` — used directly in `tvLogPath.text`. `closeCodeName(code: Int): String` used only in `onClosed`. `lastAuthOkMs`/`lastEventMs`/`skipCount` are private fields — no cross-file refs.
