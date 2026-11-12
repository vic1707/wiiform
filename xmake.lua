set_project("wiiform")
set_version("0.1.0")
set_license("GPL-2.0-only")
set_languages("cxx20")
add_rules("mode.release", "mode.debug")

includes("xmake/wiiqt-nand.lua")
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
