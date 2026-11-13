package("wiiqt-nand")
  set_description("WiiQt NAND support")
  set_homepage("https://github.com/vic1707/wiiqt-tools")
  set_license("GPL-2.0-only")
  add_urls("https://github.com/vic1707/wiiqt-tools.git")
  add_versions("0.1.3", "42536a2c3becf384687a64a3d9ddadad435dc22b")
  add_versions("0.1.4", "42536a2c3becf384687a64a3d9ddadad435dc22b")
  add_patches("0.1.3", path.join(os.scriptdir(), "patches", "wiiqt-read-only.patch"),
              "7c0eb3912a3628535d763e03528c0456467f4fac577c7639aeae67ef3396d3f6")
  add_patches("0.1.4", path.join(os.scriptdir(), "patches", "wiiqt-read-only.patch"),
              "7c0eb3912a3628535d763e03528c0456467f4fac577c7639aeae67ef3396d3f6")
  add_patches("0.1.4", path.join(os.scriptdir(), "patches", "wiiqt-buffer-device.patch"),
              "133fd2e65c4a69103d8d071dd3fc979b785d89ef10421031f5b75c16db316d18")
  add_configs("qt_sdk", {description = "Detected Qt SDK and version", type = "string", readonly = true})
  add_links("wiiqt-nand")

  on_source(function (package)
    import("detect.sdks.find_qt")
    local qt = assert(find_qt(), "Qt SDK not found; configure xmake with --qt=/path/to/qt")
    local sdk = qt.sdkdir .. "@" .. qt.sdkver
    package:extraconf_set("configs", "qt_sdk", "default", sdk)
  end)

  on_install("linux", "macosx", function (package)
    import("detect.sdks.find_qt")
    import("lib.detect.find_tool")
    local qt = assert(find_qt(), "Qt SDK not found")
    local qmake = assert(find_tool("qmake", {program = path.join(qt.bindir, "qmake")}), "Qt qmake not found")
    local builddir = package:builddir()
    os.mkdir(builddir)
    local args = {
      path.absolute("WiiQt/nand.pro"), "CONFIG-=debug_and_release",
      "SOURCES+=" .. path.absolute("WiiQt/sharedcontentmap.cpp"),
      "CONFIG-=" .. (package:is_debug() and "release" or "debug"),
      "CONFIG+=" .. (package:is_debug() and "debug" or "release")
    }
    table.insert(args, "DEFINES+=NAND_BIN_CAN_WRITE")
    os.vrunv(qmake.program, args, {curdir = builddir})
    import("package.tools.make").build(package, {}, {curdir = builddir})
    os.cp(path.join(builddir, "libwiiqt-nand.a"), package:installdir("lib"))
    os.cp("WiiQt/*.h", package:installdir("include"))
    os.cp("COPYING", package:installdir("share", "licenses", "wiiqt-nand", "COPYING"))
  end)
package_end()
