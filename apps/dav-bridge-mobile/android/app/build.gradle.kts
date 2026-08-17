plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android plugin.
    id("dev.flutter.flutter-gradle-plugin")
}

val kamoriReleaseKeystore = System.getenv("KAMORI_ANDROID_KEYSTORE_PATH")

android {
    namespace = "app.kamori.mobile"
    compileSdk = 37
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "app.kamori.mobile"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (!kamoriReleaseKeystore.isNullOrBlank()) {
            create("release") {
                storeFile = file(kamoriReleaseKeystore)
                storePassword = System.getenv("KAMORI_ANDROID_KEYSTORE_PASSWORD")
                    ?: error("KAMORI_ANDROID_KEYSTORE_PASSWORD is required for release signing")
                keyAlias = System.getenv("KAMORI_ANDROID_KEY_ALIAS")
                    ?: error("KAMORI_ANDROID_KEY_ALIAS is required for release signing")
                keyPassword = System.getenv("KAMORI_ANDROID_KEY_PASSWORD")
                    ?: error("KAMORI_ANDROID_KEY_PASSWORD is required for release signing")
            }
        }
    }

    buildTypes {
        getByName("release") {
            signingConfigs.findByName("release")?.let { signingConfig = it }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
