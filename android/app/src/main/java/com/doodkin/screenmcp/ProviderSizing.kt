package com.doodkin.screenmcp

import kotlin.math.floor
import kotlin.math.sqrt

/**
 * Provider-tuned default screenshot size, computed from the real screen dimensions.
 * See screenmcp/docs/model-sizing.md. Mirrors the desktop clients' provider_sizing.rs.
 */
object ProviderSizing {
    fun defaultSize(model: String, w: Int, h: Int): Pair<Int, Int>? {
        val wf = w.toDouble()
        val hf = h.toDouble()
        return when (model) {
            "claude" -> {
                val maxPixels = 1_176_000.0
                val maxEdge = 1568.0
                val s = minOf(1.0, maxEdge / maxOf(wf, hf), sqrt(maxPixels / (wf * hf)))
                var mw = floor(wf * s).toInt()
                var mh = floor(hf * s).toInt()
                while (mw.toLong() * mh.toLong() > maxPixels.toLong() && (mw > 1 || mh > 1)) {
                    if (mw >= mh) mw-- else mh--
                }
                Pair(mw, mh)
            }
            "gemini" -> {
                val shortCap = 1080.0
                val longCap = 1920.0
                val s = if (w >= h) minOf(1.0, longCap / wf, shortCap / hf)
                        else minOf(1.0, longCap / hf, shortCap / wf)
                Pair(Math.round(wf * s).toInt(), Math.round(hf * s).toInt())
            }
            "chatgpt" -> {
                val short = minOf(wf, hf)
                var s = minOf(1.0, 768.0 / short)
                val long = maxOf(wf, hf)
                if (long * s > 2048.0) s = 2048.0 / long
                Pair(round16(wf * s), round16(hf * s))
            }
            else -> null
        }
    }

    private fun round16(x: Double): Int = Math.max(16, (Math.round(x / 16.0) * 16).toInt())
}
