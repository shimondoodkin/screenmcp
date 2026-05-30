package com.doodkin.screenmcp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ProviderSizingTest {
    // (screen w,h) -> expected (claude, gemini, chatgpt). Canonical vectors, identical to
    // the desktop clients' provider_sizing.rs test table.
    private data class Vec(
        val w: Int, val h: Int,
        val claude: Pair<Int, Int>,
        val gemini: Pair<Int, Int>,
        val chatgpt: Pair<Int, Int>,
    )

    private val vectors = listOf(
        Vec(2560, 1440, 1445 to 813, 1920 to 1080, 1360 to 768),
        Vec(1920, 1080, 1445 to 813, 1920 to 1080, 1360 to 768),
        Vec(3840, 2160, 1445 to 813, 1920 to 1080, 1360 to 768),
        Vec(1080, 2400, 705 to 1568, 864 to 1920, 768 to 1712),
        Vec(1440, 3120, 723 to 1568, 886 to 1920, 768 to 1664),
        Vec(1080, 3000, 564 to 1567, 691 to 1920, 736 to 2048),
        Vec(1000, 1000, 1000 to 1000, 1000 to 1000, 768 to 768),
        Vec(640, 480, 640 to 480, 640 to 480, 640 to 480),
    )

    @Test
    fun matchesCanonicalTable() {
        for (v in vectors) {
            assertEquals("claude ${v.w}x${v.h}", v.claude, ProviderSizing.defaultSize("claude", v.w, v.h))
            assertEquals("gemini ${v.w}x${v.h}", v.gemini, ProviderSizing.defaultSize("gemini", v.w, v.h))
            assertEquals("chatgpt ${v.w}x${v.h}", v.chatgpt, ProviderSizing.defaultSize("chatgpt", v.w, v.h))
        }
    }

    @Test
    fun unknownModelReturnsNull() {
        assertNull(ProviderSizing.defaultSize("gpt-5", 1920, 1080))
    }
}
