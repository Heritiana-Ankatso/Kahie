package app.kahie

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.File

/**
 * Serves files from `cacheDir/share/` to the app chosen in the share sheet (P1-36), read-only
 * and only through URIs granted with FLAG_GRANT_READ_URI_PERMISSION. A minimal stand-in for
 * AndroidX FileProvider, which the app does not otherwise need.
 */
class ShareProvider : ContentProvider() {
    companion object {
        const val AUTHORITY = "app.kahie.share"

        fun dir(context: android.content.Context) = File(context.cacheDir, "share").apply { mkdirs() }

        fun uriFor(file: File): Uri = Uri.Builder().scheme("content").authority(AUTHORITY).appendPath(file.name).build()
    }

    private fun file(uri: Uri): File {
        val name = uri.lastPathSegment ?: throw java.io.FileNotFoundException()
        val dir = dir(context!!)
        val f = File(dir, name)
        // Only plain names inside share/: no path traversal.
        if (f.parentFile?.canonicalPath != dir.canonicalPath || !f.isFile) throw java.io.FileNotFoundException(name)
        return f
    }

    override fun onCreate() = true

    override fun getType(uri: Uri): String = when (val ext = uri.lastPathSegment?.substringAfterLast('.')?.lowercase()) {
        "pdf" -> "application/pdf"
        "png" -> "image/png"
        "jpg", "jpeg" -> "image/jpeg"
        "svg" -> "image/svg+xml"
        else -> ext?.let { android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(it) } ?: "application/octet-stream"
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        // Writable only for a camera capture the app asked for (P1-18: insert from camera).
        if (mode != "r") {
            if (uri.lastPathSegment?.startsWith("capture-") != true) throw SecurityException("read-only")
            return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.parseMode(mode))
        }
        return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun query(uri: Uri, projection: Array<String>?, selection: String?, args: Array<String>?, sort: String?): Cursor {
        val f = file(uri)
        val cols = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        val row = cols.map { c ->
            when (c) {
                OpenableColumns.DISPLAY_NAME -> f.name
                OpenableColumns.SIZE -> f.length()
                else -> null
            }
        }
        return MatrixCursor(cols, 1).apply { addRow(row) }
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<String>?) = 0
    override fun delete(uri: Uri, selection: String?, args: Array<String>?) = 0
}
