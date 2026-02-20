package com.phonemcp.app

import android.accessibilityservice.AccessibilityService
import android.graphics.Bitmap
import java.io.File
import java.io.FileOutputStream

class ScreenshotManager(private val cacheDir: File) {

    fun takeScreenshot(
        service: PhoneMcpService,
        onResult: (Result<File>) -> Unit
    ) {
        service.takeScreenshot(object : AccessibilityService.TakeScreenshotCallback {
            override fun onSuccess(screenshot: AccessibilityService.ScreenshotResult) {
                try {
                    val hwBuffer = screenshot.hardwareBuffer
                    val colorSpace = screenshot.colorSpace
                    val bitmap = Bitmap.wrapHardwareBuffer(hwBuffer, colorSpace)
                        ?: throw IllegalStateException("Failed to create bitmap from hardware buffer")

                    val softBitmap = bitmap.copy(Bitmap.Config.ARGB_8888, false)
                    bitmap.recycle()
                    hwBuffer.close()

                    val file = File(cacheDir, "screenshot_${System.currentTimeMillis()}.png")
                    FileOutputStream(file).use { out ->
                        softBitmap.compress(Bitmap.CompressFormat.PNG, 90, out)
                    }
                    softBitmap.recycle()

                    onResult(Result.success(file))
                } catch (e: Exception) {
                    onResult(Result.failure(e))
                }
            }

            override fun onFailure(errorCode: Int) {
                onResult(Result.failure(RuntimeException("Screenshot failed with error code: $errorCode")))
            }
        })
    }
}
