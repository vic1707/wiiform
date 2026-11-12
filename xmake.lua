set_project("wiiform")
set_version("0.1.0")
set_license("GPL-2.0-only")
set_languages("cxx20")
add_rules("mode.release", "mode.debug")

includes("xmake/wiiqt-nand.lua")
includes("xmake/priiloader.lua")
add_requires("yaml-cpp 0.8.0", "xz 5.8.4")
add_requires("wiiqt-nand 0.1.2")
add_requires("wiiqt-nand~writable 0.1.2", {alias = "wiiqt-writable", configs = {writable = true}})

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
    add_includedirs("src-wiiformlib/include", {public = true})
    add_headerfiles("src-wiiformlib/include/(wiiform/*.hpp)")
    add_includedirs("src-wiiformlib/src/common", "src-wiiformlib/src/nand",
                    "src-wiiformlib/src/preflight", "src-wiiformlib/src/seed")
    add_files("src-wiiformlib/src/common/*.cpp",
              "src-wiiformlib/src/seed/Seed.cpp", "src-wiiformlib/src/nand/Stock.cpp", "src-wiiformlib/src/nand/Create.cpp", "src-wiiformlib/src/nand/Nus.cpp",
              "src-wiiformlib/src/nand/Install.cpp", "src-wiiformlib/src/nand/Remove.cpp", "src-wiiformlib/src/nand/WadPackage.cpp",
              "src-wiiformlib/src/nand/WorkingImage.cpp", "src-wiiformlib/src/nand/Display.cpp",
              "src-wiiformlib/src/nand/Priiloader.cpp", "src-wiiformlib/src/core/write.cpp", "src-wiiformlib/src/core/BackupMii.cpp")
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
    add_includedirs("src-wiiformlib/include", {public = true})
    add_headerfiles("src-wiiformlib/include/(wiiform/*.hpp)")
    add_includedirs("src-wiiformlib/src/common", "src-wiiformlib/src/nand",
                    "src-wiiformlib/src/preflight", "src-wiiformlib/src/seed")
    add_packages("yaml-cpp", "xz")
    add_files("src-wiiformlib/src/core/ReadApi.cpp",
              "src-wiiformlib/src/common/BootDiagnostics.cpp", "src-wiiformlib/src/common/BuildOptions.cpp",
              "src-wiiformlib/src/common/Inspection.cpp", "src-wiiformlib/src/common/Layout.cpp",
              "src-wiiformlib/src/common/PriiloaderHacks.cpp", "src-wiiformlib/src/common/PriiloaderSettings.cpp",
              "src-wiiformlib/src/common/SysConf.cpp", "src-wiiformlib/src/common/TitleValidation.cpp",
              "src-wiiformlib/src/nand/Stock.cpp", "src-wiiformlib/src/nand/WadPackage.cpp",
              "src-wiiformlib/src/preflight/Inventory.cpp", "src-wiiformlib/src/preflight/Preflight.cpp", "src-wiiformlib/src/seed/Seed.cpp")
    add_cxxflags("-Wall", "-Wextra", "-pedantic")
    set_targetdir("build/lib")
target_end()

target("wiiform-readonly")
    add_rules("qt.console")
    add_deps("wiiform-core-readonly")
    add_frameworks("QtGui", "QtWidgets")
    add_packages("wiiqt-nand")
    add_includedirs("src-wiiformlib/include")
    add_includedirs("src-wiiformlib/src/common", "src-wiiformlib/src/nand",
                    "src-wiiformlib/src/preflight", "src-wiiformlib/src/seed")
    add_packages("yaml-cpp", "xz")
    add_files("src-wiiformlib/src/core/worker_main.cpp")
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
        add_includedirs("src-wiiformlib/src/common", "src-wiiformlib/src/nand",
                        "src-wiiformlib/src/preflight", "src-wiiformlib/src/seed")
        add_files("tests/" .. name .. ".cpp")
        set_targetdir("build/bin")
        set_rundir("$(projectdir)")
    target_end()
end

target("public-api-tests")
    set_kind("binary")
    set_default(false)
    add_deps("wiiform-core-write")
    add_files("tests/public_api.cpp")
    set_targetdir("build/bin")
target_end()
