set_project("wiiform")
set_version("0.1.0")
set_license("GPL-2.0-only")
set_languages("cxx20")
add_rules("mode.release", "mode.debug")

includes("xmake/wiiqt-nand.lua")
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
    add_includedirs("include", {public = true})
    add_includedirs("inc")
    add_files("src/smoke/writable_core.cpp", "src/common/*.cpp",
              "src/seed/Seed.cpp", "src/nand/Stock.cpp", "src/nand/Create.cpp", "src/nand/Nus.cpp",
              "src/nand/Install.cpp", "src/nand/Remove.cpp", "src/nand/WadPackage.cpp",
              "src/nand/WorkingImage.cpp")
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
    add_includedirs("include", {public = true})
    add_includedirs("inc")
    add_packages("yaml-cpp", "xz")
    add_files("src/smoke/readonly_core.cpp", "src/core/ReadApi.cpp",
              "src/common/BootDiagnostics.cpp", "src/common/BuildOptions.cpp",
              "src/common/Inspection.cpp", "src/common/Layout.cpp",
              "src/common/PriiloaderHacks.cpp", "src/common/PriiloaderSettings.cpp",
              "src/common/SysConf.cpp", "src/common/TitleValidation.cpp",
              "src/nand/Stock.cpp", "src/nand/WadPackage.cpp",
              "src/preflight/Inventory.cpp", "src/preflight/Preflight.cpp", "src/seed/Seed.cpp")
    add_cxxflags("-Wall", "-Wextra", "-pedantic")
    set_targetdir("build/lib")
target_end()

target("wiiform-readonly")
    add_rules("qt.console")
    add_deps("wiiform-core-readonly")
    add_frameworks("QtGui", "QtWidgets")
    add_packages("wiiqt-nand")
    add_includedirs("include")
    add_includedirs("inc")
    add_packages("yaml-cpp", "xz")
    add_files("src/core/worker_main.cpp")
    add_cxxflags("-Wall", "-Wextra", "-pedantic")
    set_targetdir("build/libexec")
    if is_plat("macosx") then
        add_ldflags("-Wl,-headerpad_max_install_names")
    end
target_end()
