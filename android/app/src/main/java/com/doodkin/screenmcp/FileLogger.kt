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
