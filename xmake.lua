set_project("wiiform")
set_version("0.1.0")
set_license("GPL-2.0-only")
set_languages("cxx20")
add_rules("mode.release", "mode.debug")

includes("xmake/wiiqt-nand.lua")
includes("xmake/priiloader.lua")
add_requires("wiiqt-nand 0.2.0")

local private_headers = {
    "wiiform-lib/src/common",
    "wiiform-lib/src/nand",
    "wiiform-lib/src/seed"
}

target("wiiform-core")
    add_rules("qt.shared")
    add_frameworks("QtGui", "QtWidgets", "QtNetwork")
    add_packages("wiiqt-nand")
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

for name, source in pairs({
    ["nand-buffer-tests"] = "tests/nand_buffer.cpp",
    ["nand-buffer-regressions"] = "tests/nand_buffer_regressions.cpp",
    ["sysconf-generated-tests"] = "tests/sysconf_generated.cpp"
}) do
    target(name)
        set_kind("binary")
        set_default(false)
        add_rules("qt.console")
        add_deps("wiiform-core")
        add_frameworks("QtGui", "QtWidgets", "QtNetwork")
        add_packages("wiiqt-nand")
        add_includedirs(private_headers)
        add_files(source)
        set_targetdir("build/bin")
    target_end()
end
