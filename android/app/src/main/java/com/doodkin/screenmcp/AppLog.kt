package com.doodkin.screenmcp

import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Global app-wide log buffer visible in MainActivity's log text view.
 * All services (FCM, SSE, ConnectionService) write here.
 */
object AppLog {
    private val entries = mutableListOf<String>()
    @Volatile var version = 0L
        private set

    fun add(tag: String, msg: String) {
        val time = SimpleDateFormat("HH:mm:ss.SSS", Locale.getDefault()).format(Date())
        synchronized(entries) {
            entries.add("[$time][$tag] $msg")
            if (entries.size > 200) entries.removeAt(0)
            version++
        }
    }

    fun getEntries(): List<String> = synchronized(entries) { entries.toList() }
}
