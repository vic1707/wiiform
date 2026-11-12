set_project("wiiform")
set_version("0.1.0")
set_license("GPL-2.0-only")
set_languages("cxx20")
add_rules("mode.release", "mode.debug")

includes("xmake/wiiqt-nand.lua")
includes("xmake/priiloader.lua")
add_requires("yaml-cpp 0.8.0", "xz 5.8.4")
add_requires("wiiqt-nand 0.1.1")
add_requires("wiiqt-nand~writable 0.1.1", {alias = "wiiqt-writable", configs = {writable = true}})

for name, package_name in pairs({["check-wiiqt-read"] = "wiiqt-nand", ["check-wiiqt-write"] = "wiiqt-writable"}) do
    target(name)
        set_default(false)
        add_rules("qt.console")
        add_frameworks("QtGui", "QtWidgets")
        add_packages(package_name)
        add_files("tests/wiiqt_link.cpp")
        set_targetdir("build/bin")
    target_end()
end

target("wiiform-core-write")
    add_rules("qt.shared")
    add_frameworks("QtGui", "QtWidgets", "QtNetwork")
    add_packages("wiiqt-writable", "yaml-cpp", "xz")
    add_includedirs("native/include", {public = true})
    add_headerfiles("native/include/(wiiform/*.hpp)")
    add_includedirs("native/src")
    add_files("native/src/common/*.cpp",
              "native/src/seed/Seed.cpp", "native/src/nand/Stock.cpp", "native/src/nand/Create.cpp", "native/src/nand/Nus.cpp",
              "native/src/nand/Install.cpp", "native/src/nand/Remove.cpp", "native/src/nand/WadPackage.cpp",
              "native/src/nand/WorkingImage.cpp", "native/src/nand/Display.cpp",
              "native/src/nand/Priiloader.cpp", "native/src/core/write.cpp", "native/src/core/BackupMii.cpp")
    add_cxxflags("-Wall", "-Wextra", "-pedantic")
    set_targetdir("build/lib")
    if is_plat("macosx") then
        add_shflags("-Wl,-headerpad_max_install_names")
    end
target_end()

target("wiiform-core-readonly")
    add_rules("qt.static")
    add_frameworks("QtGui", "QtWidgets")
    add_packages("wiiqt-nand")
    add_includedirs("native/include", {public = true})
    add_headerfiles("native/include/(wiiform/*.hpp)")
    add_includedirs("native/src")
    add_packages("yaml-cpp", "xz")
    add_files("native/src/core/ReadApi.cpp",
              "native/src/common/BootDiagnostics.cpp", "native/src/common/BuildOptions.cpp",
              "native/src/common/Inspection.cpp", "native/src/common/Layout.cpp",
              "native/src/common/PriiloaderHacks.cpp", "native/src/common/PriiloaderSettings.cpp",
              "native/src/common/SysConf.cpp", "native/src/common/TitleValidation.cpp",
              "native/src/nand/Stock.cpp", "native/src/nand/WadPackage.cpp",
              "native/src/preflight/Inventory.cpp", "native/src/preflight/Preflight.cpp", "native/src/seed/Seed.cpp")
    add_cxxflags("-Wall", "-Wextra", "-pedantic")
    set_targetdir("build/lib")
target_end()

target("wiiform-readonly")
    add_rules("qt.console")
    add_deps("wiiform-core-readonly")
    add_frameworks("QtGui", "QtWidgets")
    add_packages("wiiqt-nand")
    add_includedirs("native/include")
    add_includedirs("native/src")
    add_packages("yaml-cpp", "xz")
    add_files("native/src/core/worker_main.cpp")
    add_cxxflags("-Wall", "-Wextra", "-pedantic")
    set_targetdir("build/libexec")
    if is_plat("macosx") then
        add_ldflags("-Wl,-headerpad_max_install_names")
    end
target_end()

for _, name in ipairs({"install", "inventory", "preflight", "remove", "roundtrip", "sysconf", "wad", "backupmii_footer"}) do
    target(name .. "-tests")
        set_default(false)
        add_rules("qt.console")
        add_deps("wiiform-core-write")
        add_frameworks("QtGui", "QtWidgets", "QtNetwork")
        add_packages("wiiqt-writable", "yaml-cpp", "xz")
        add_includedirs("native/src", "native/include")
        add_files("tests/" .. name .. ".cpp")
        set_targetdir("build/bin")
        set_rundir("$(projectdir)")
    target_end()
end
