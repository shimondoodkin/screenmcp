# Android Client File Logging for Reconnection Debugging

## Problem

The Android client occasionally "blinks" — visibly connecting, disconnecting, and reconnecting in rapid succession. Existing logs are kept only in `AppLog` (in-memory, ~200 entries) and disappear when the app process is killed or the buffer wraps. We cannot pull a clean log after a blink episode for post-mortem analysis.

## Goal

Persist all `AppLog` output to a rotating file on the device when the user opts in via a checkbox in `MainActivity`. Add a small amount of extra diagnostic context around connect/disconnect to make the blink cause visible in the file.

## Non-Goals

- No remote log upload, telemetry, or crash reporting.
- No structured logging format (JSON, etc.) — keep human-readable.
- No new logging library — write directly to a file.
- No changes to other clients (Mac/Windows/Linux).

## Architecture

```
WebSocketClient ─┐
SseService       ├──► AppLog.add(tag, msg) ──┬─► in-memory buffer (200 lines, existing)
ConnectionService├                            │
ScreenMcpService │                            └─► FileLogger.log(tag, msg) ──► HandlerThread
MainActivity (UI)┘                                                              │
                                                                                ▼
                                                                       <ext-files>/logs/
                                                                          screenmcp.log
                                                                          screenmcp.1.log
                                                                          screenmcp.2.log
```

Single chokepoint: `AppLog.add()` already receives every log line in the app. Adding one call into `FileLogger.log()` at that point captures all existing tags (`WS`, `SSE`, `Conn`, `UI`) with no per-call-site changes.

`FileLogger` is a process-wide singleton owning all file I/O. Writes are dispatched to a dedicated `HandlerThread` so callers of `AppLog.add()` never block on disk I/O.

## Components

### `FileLogger` (new — `android/app/src/main/java/com/doodkin/screenmcp/FileLogger.kt`)

Public API:

```kotlin
object FileLogger {
    fun init(context: Context)               // call once from Application/MainActivity onCreate
    fun setEnabled(context: Context, on: Boolean)
    fun isEnabled(): Boolean
    fun log(tag: String, msg: String)        // no-op if disabled or not initialized
    fun clear()                              // delete all 3 log files
    fun getLogDir(): File                    // <external-files>/logs/
    fun getLogDirPath(): String              // string for UI display
}
```

Internals:
- One `HandlerThread("FileLogger")` started lazily on first `setEnabled(true)`; stopped when disabled.
- `BufferedWriter` over `FileOutputStream(file, append=true)`; flush after each line for crash-safety.
- Persist enabled state in SharedPreferences key `file_logging_enabled` (in the existing `screenmcp` prefs file).
- All file ops happen on the HandlerThread; thread-safety via single-threaded execution rather than locks.

### Rotation

- Path: `<context.getExternalFilesDir(null)>/logs/`
- Files: `screenmcp.log` (current), `screenmcp.1.log`, `screenmcp.2.log` (oldest)
- Max per-file: **10 MB** (`10 * 1024 * 1024` bytes)
- Total budget: **30 MB**
- Check size before each write. When current ≥ 10 MB:
  1. Delete `screenmcp.2.log` if exists.
  2. Rename `screenmcp.1.log` → `screenmcp.2.log` if exists.
  3. Close current writer; rename `screenmcp.log` → `screenmcp.1.log`.
  4. Open fresh `screenmcp.log` writer.
- A rotation event itself logs a single marker line `--- rotated at <timestamp> ---` to the new file.

### Line format

`YYYY-MM-DD HH:mm:ss.SSS [TAG] message\n` — UTF-8.

Example:
```
2026-05-12 09:21:14.483 [SSE] Event: type=connect
2026-05-12 09:21:14.491 [Conn] Connecting → wss://...
2026-05-12 09:21:14.501 [WS] [gen=3] WS connecting to wss://...
2026-05-12 09:21:14.612 [WS] [gen=3] WS opened in 111ms, sending auth
2026-05-12 09:21:14.701 [WS] [gen=3] Authenticated (total connect: 218ms)
```

### `AppLog` modification

One additional line in `AppLog.add()`:

```kotlin
fun add(tag: String, msg: String) {
    val time = SimpleDateFormat("HH:mm:ss.SSS", Locale.getDefault()).format(Date())
    synchronized(entries) {
        entries.add("[$time][$tag] $msg")
        if (entries.size > 200) entries.removeAt(0)
        version++
    }
    FileLogger.log(tag, msg)   // <-- new
}
```

`FileLogger.log()` itself handles the disabled/uninitialized cases and dispatches the date-formatted full-precision line to its own thread, so this call is cheap.

### UI changes — `activity_main.xml`

Insert this block just above the existing `tvLog` TextView at the bottom of the main `LinearLayout`:

```xml
<!-- Debug File Logging -->
<View android:layout_width="match_parent" android:layout_height="1dp"
      android:background="#CCCCCC" android:layout_marginVertical="8dp" />

<CheckBox android:id="@+id/cbFileLogging"
          android:layout_width="match_parent" android:layout_height="wrap_content"
          android:text="Write debug log to file" />

<TextView android:id="@+id/tvLogPath"
          android:layout_width="match_parent" android:layout_height="wrap_content"
          android:textSize="10sp" android:fontFamily="monospace"
          android:textColor="#666666"
          android:visibility="gone"
          android:layout_marginBottom="4dp" />

<LinearLayout android:layout_width="match_parent" android:layout_height="wrap_content"
              android:orientation="horizontal"
              android:layout_marginBottom="8dp">
    <Button android:id="@+id/btnOpenLogsFolder"
            android:layout_width="0dp" android:layout_height="wrap_content"
            android:layout_weight="1"
            android:text="Open Logs Folder" android:layout_marginEnd="4dp" />
    <Button android:id="@+id/btnClearLogs"
            android:layout_width="0dp" android:layout_height="wrap_content"
            android:layout_weight="1"
            android:text="Clear Logs" />
</LinearLayout>
```

### `MainActivity` wiring

In `onCreate`:
1. Call `FileLogger.init(this)` early.
2. Find `cbFileLogging`, `tvLogPath`, `btnOpenLogsFolder`, `btnClearLogs`.
3. Set `cbFileLogging.isChecked = FileLogger.isEnabled()`; update `tvLogPath` visibility/text.
4. `cbFileLogging.setOnCheckedChangeListener { _, checked -> FileLogger.setEnabled(this, checked); tvLogPath.visibility = if (checked) View.VISIBLE else View.GONE; tvLogPath.text = "Path: " + FileLogger.getLogDirPath() }`.
5. `btnOpenLogsFolder.setOnClickListener` — try `Intent(Intent.ACTION_VIEW)` with a `FileProvider`-backed URI for the folder; if no resolver, show the path in a Toast as fallback.
6. `btnClearLogs.setOnClickListener` — `FileLogger.clear()` + Toast confirmation.

`FileProvider` requires a `<provider>` entry in `AndroidManifest.xml` and an `xml/file_paths.xml` resource exposing `<external-files-path name="logs" path="logs/" />`. If no file-manager app resolves the intent, the button falls back to showing the path string in a Toast.

## Diagnostic Context Additions

These extra `tlog`/`AppLog.add` calls in the existing code paths make the blink cause visible in the file log. Each is one short line.

### `WebSocketClient.kt`
- Include `[gen=N]` prefix in every `tlog()` (where N is `connectionGeneration`) — exposes stale-callback patterns.
- New log on `"ping"` message received and pong sent: `"ping received, pong sent"`.
- In `onClosed`/`onFailure`, append elapsed time since last `auth_ok` (track `lastAuthOkMs` as a new field).
- In `onClosed`, append a human-readable close-code name:
  - 1000 normal, 1001 going_away, 1002 protocol_error, 1006 abnormal, 1011 server_error, 4001+ app-defined.

### `ConnectionService.kt`
- Count "Already connected to X, skipping" hits since last successful connect (new field `skipCount`); log includes the count: `"Already connected to X, skipping (skip #4 since last connect)"`.
- Log `onCreate` and `onDestroy` of `ConnectionService` itself — `"ConnectionService onCreate"` / `"onDestroy"`. Lifecycle churn is a probable blink cause.

### `SseService.kt`
- Track `lastEventMs`; on each SSE event log gap: `"Event: type=connect (gap 12.3s since last event)"`.
- On `onClosed`/`onFailure` of the SSE source, if `ScreenMcpService.instance?.isWorkerConnected() == true`, log a warning: `"SSE dropped while WS still up"`.

### `ScreenMcpService.kt`
- Log on `onServiceConnected()` and `onDestroy()` — accessibility service restarts kill the WS.

All additions go through the existing `AppLog.add()` / `tlog()` path, so they appear in both the in-memory and file logs.

## Data Flow

Connect cycle, file-logging enabled:
```
SSE event arrives
  → SseService.handleConnectEvent → AppLog.add("SSE", "Event: type=connect (gap 0.3s)")
      → FileLogger.log enqueues to HandlerThread → writes line to screenmcp.log
  → SseService starts ConnectionService → AppLog.add("Conn", "ConnectionService onCreate")
  → ConnectionService.onStartCommand → checks isWorkerConnectedTo
      if true:  AppLog.add("Conn", "Already connected, skipping (skip #5 since last connect)")
      if false: AppLog.add("Conn", "Connecting → wsUrl")
  → WebSocketClient.tlog("[gen=N] WS connecting...") → AppLog → FileLogger
  → ... open / auth_ok / closed all logged with gen + elapsed
```

## Error handling

- `FileLogger.log()` swallows all `IOException` — logging must never crash the app. On write failure, set a `disabledByError` flag for the rest of the session and emit one logcat `Log.w` (no recursion into AppLog).
- If `getExternalFilesDir(null)` returns null (no external storage), file logging silently no-ops; checkbox stays checked but Toast on enable warns.
- Rotation file-rename failures are tolerated — worst case the next write happens to the existing oversized file (size check is best-effort).

## Testing

- **Unit-style sanity** (manual): toggle checkbox, run an SSE-triggered reconnect, pull the file with `adb pull /sdcard/Android/data/com.doodkin.screenmcp/files/logs/screenmcp.log`, confirm timestamps and tags.
- **Rotation test** (manual or instrumented): set `MAX_BYTES` to e.g. 10 KB via a debug-only entry point, hammer `AppLog.add()` 1000× and confirm 3-file rotation state.
- **Disabled-path perf**: confirm `FileLogger.log()` returns in <100 ns when disabled (single volatile read).
- **Verification scenario**: with logging on, trigger a real blink (force-stop + restart, or pull network cable). Open the file and confirm the timeline is reconstructable: SSE event → ConnectionService onCreate → WS open → auth_ok → WS closed with code → reconnect attempt.

## Files Touched

New:
- `android/app/src/main/java/com/doodkin/screenmcp/FileLogger.kt`
- `android/app/src/main/res/xml/file_paths.xml` (FileProvider config)

Modified:
- `android/app/src/main/java/com/doodkin/screenmcp/AppLog.kt` — one new line.
- `android/app/src/main/java/com/doodkin/screenmcp/MainActivity.kt` — init + UI wiring.
- `android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt` — gen prefix, ping/pong log, close-code names, lastAuthOkMs.
- `android/app/src/main/java/com/doodkin/screenmcp/ConnectionService.kt` — skipCount, onCreate/onDestroy logs.
- `android/app/src/main/java/com/doodkin/screenmcp/SseService.kt` — event gap, "SSE dropped while WS up".
- `android/app/src/main/java/com/doodkin/screenmcp/ScreenMcpService.kt` — accessibility lifecycle logs.
- `android/app/src/main/res/layout/activity_main.xml` — checkbox + path TextView + 2 buttons above existing `tvLog`.
- `android/app/src/main/AndroidManifest.xml` — `<provider>` entry for FileProvider.
