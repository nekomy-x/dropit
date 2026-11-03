pluginManagement {
    val flutterSdkPath =
        run {
            val localProperties = file("local.properties")
            val contents = localProperties.readText(Charsets.UTF_8)
            // Flutter writes UTF-8, while its plugin loader uses Java properties
            // decoding. Escape Unicode once so every SDK reader sees the same path.
            if (contents.any { it.code > 0x7f }) {
                localProperties.writeText(buildString {
                    contents.forEach { character ->
                        if (character.code > 0x7f) {
                            append("\\u")
                            append(character.code.toString(16).padStart(4, '0'))
                        } else {
                            append(character)
                        }
                    }
                }, Charsets.US_ASCII)
            }
            val properties = java.util.Properties()
            localProperties.inputStream().use { properties.load(it) }
            val flutterSdkPath = properties.getProperty("flutter.sdk")
            require(flutterSdkPath != null) { "flutter.sdk not set in local.properties" }
            flutterSdkPath
        }

    includeBuild("$flutterSdkPath/packages/flutter_tools/gradle")

    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

plugins {
    id("dev.flutter.flutter-plugin-loader") version "1.0.0"
    id("com.android.application") version "9.1.0" apply false
    id("org.jetbrains.kotlin.android") version "2.4.0" apply false
}

include(":app")
