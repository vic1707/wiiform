set_project("wiiform")
set_version("0.1.0")
set_license("GPL-2.0-only")
set_languages("cxx20")
add_rules("mode.release", "mode.debug")

includes("xmake/wiiqt-nand.lua")
includes("xmake/priiloader.lua")
add_requires("yaml-cpp 0.8.0", "xz 5.8.4")
add_requires("wiiqt-nand 0.1.5")

local private_headers = {
    "wiiform-lib/src/common",
    "wiiform-lib/src/nand",
    "wiiform-lib/src/seed"
}

target("wiiform-core")
    add_rules("qt.shared")
    add_frameworks("QtGui", "QtWidgets", "QtNetwork")
    add_packages("wiiqt-nand", "yaml-cpp", "xz")
    add_includedirs("wiiform-lib/include", {public = true})
    add_headerfiles("wiiform-lib/include/(wiiform/*.hpp)")
    add_includedirs(private_headers)
    add_files("wiiform-lib/src/**/*.cpp")
    add_cxxflags("-Wall", "-Wextra", "-pedantic")
    set_targetdir("build/lib")
    if is_plat("macosx") then
        add_shflags("-Wl,-headerpad_max_install_names")
    end
target_end()

for _, name in ipairs({"install", "inventory", "preflight", "remove", "roundtrip", "sysconf", "wad", "backupmii_footer"}) do
    target(name .. "-tests")
        set_default(false)
        add_rules("qt.console")
        add_deps("wiiform-core")
        add_frameworks("QtGui", "QtWidgets", "QtNetwork")
        add_packages("wiiqt-nand", "yaml-cpp", "xz")
        add_includedirs(private_headers)
        add_files("tests/" .. name .. ".cpp")
        set_targetdir("build/bin")
        set_rundir("$(projectdir)")
    target_end()
end

target("public-api-tests")
    set_kind("binary")
    set_default(false)
    add_deps("wiiform-core")
    add_files("tests/public_api.cpp")
    set_targetdir("build/bin")
target_end()

target("nand-buffer-tests")
    set_kind("binary")
    set_default(false)
    add_rules("qt.console")
    add_deps("wiiform-core")
    add_frameworks("QtGui", "QtWidgets", "QtNetwork")
    add_packages("wiiqt-nand", "yaml-cpp", "xz")
    add_includedirs(private_headers)
    add_files("tests/nand_buffer.cpp")
    set_targetdir("build/bin")
target_end()
