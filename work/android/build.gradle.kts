allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

val newBuildDir: Directory =
    rootProject.layout.buildDirectory
        .dir("../../build")
        .get()
rootProject.layout.buildDirectory.value(newBuildDir)

subprojects {
    val newSubprojectBuildDir: Directory = newBuildDir.dir(project.name)
    project.layout.buildDirectory.value(newSubprojectBuildDir)
}
subprojects {
    project.evaluationDependsOn(":app")
}

// bonsoir_android 等插件模块自身 compileSdk 偏低（android-33），其依赖的
// androidx 库（fragment 1.7.x 等）要求 34+——统一提升到 36（SDK 已安装）。
// :app 已在自身 build.gradle.kts 显式设 36，这里只处理插件模块。
subprojects {
    if (name == "app") return@subprojects
    afterEvaluate {
        when (val ext = extensions.findByName("android")) {
            is com.android.build.api.dsl.LibraryExtension -> ext.compileSdk = 36
            is com.android.build.api.dsl.ApplicationExtension -> ext.compileSdk = 36
        }
    }
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
