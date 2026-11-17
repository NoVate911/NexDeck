package org.example.nexdeck_mobile

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    companion object {
        private const val STORAGE_CHANNEL = "org.example.nexdeck_mobile/storage"
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, STORAGE_CHANNEL)
            .setMethodCallHandler { call, result ->
                if (call.method == "getAppDataDir") {
                    result.success(filesDir.absolutePath)
                } else {
                    result.notImplemented()
                }
            }
    }
}
