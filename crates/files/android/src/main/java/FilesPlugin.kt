// Files the phone exchanges with the computer through Yandex Disk (D39). The picker asks to
// keep the access to the picked file, so the app reads and writes it after a restart too.

package app.reestr.files

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import kotlin.concurrent.thread

@InvokeArg
class CopyArgs {
    lateinit var uri: String
    lateinit var path: String
}

@TauriPlugin
class FilesPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun pick(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "pickResult")
    }

    @ActivityCallback
    fun pickResult(invoke: Invoke, result: ActivityResult) {
        val res = JSObject()
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            res.put("uri", null)
            invoke.resolve(res)
            return
        }
        keepAccess(uri, result.data?.flags ?: 0)
        res.put("uri", uri.toString())
        res.put("name", displayName(uri))
        invoke.resolve(res)
    }

    /** Keeps reading and, when the provider allows it, writing after a restart. */
    private fun keepAccess(uri: Uri, granted: Int) {
        val resolver = activity.contentResolver
        val wanted = granted and
            (Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        try {
            resolver.takePersistableUriPermission(uri, wanted)
        } catch (e: SecurityException) {
            try {
                resolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            } catch (ignored: SecurityException) {
                // The provider does not keep access: the file is picked again next time.
            }
        }
    }

    private fun displayName(uri: Uri): String? {
        activity.contentResolver
            .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst()) return cursor.getString(0)
            }
        return null
    }

    // A cloud provider may download or upload while the stream is open: off the main thread.

    @Command
    fun copyFrom(invoke: Invoke) {
        val args = invoke.parseArgs(CopyArgs::class.java)
        thread {
            try {
                val input = activity.contentResolver.openInputStream(Uri.parse(args.uri))
                    ?: throw java.io.IOException("cannot open ${args.uri}")
                input.use { i -> FileOutputStream(File(args.path)).use { o -> i.copyTo(o) } }
                invoke.resolve()
            } catch (e: Exception) {
                invoke.reject(e.message ?: "copy failed")
            }
        }
    }

    @Command
    fun copyTo(invoke: Invoke) {
        val args = invoke.parseArgs(CopyArgs::class.java)
        thread {
            try {
                val output = activity.contentResolver.openOutputStream(Uri.parse(args.uri), "wt")
                    ?: throw java.io.IOException("cannot write ${args.uri}")
                output.use { o -> FileInputStream(File(args.path)).use { i -> i.copyTo(o) } }
                invoke.resolve()
            } catch (e: Exception) {
                invoke.reject(e.message ?: "copy failed")
            }
        }
    }
}
