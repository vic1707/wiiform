package("wiiqt-nand")
  set_description("WiiQt NAND support")
  set_homepage("https://github.com/vic1707/wiiqt-tools")
  set_license("GPL-2.0-only")
  -- WIIQT_GIT_URL points the build at a local checkout of the fork while developing against it.
  add_urls(os.getenv("WIIQT_GIT_URL") or "https://github.com/vic1707/wiiqt-tools.git")
  add_versions("0.3.0", "deb2270838955f617ebc2ef200da7c794cd18ef6")
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
      "CONFIG-=" .. (package:is_debug() and "release" or "debug"),
      "CONFIG+=" .. (package:is_debug() and "debug" or "release")
    }
    os.vrunv(qmake.program, args, {curdir = builddir})
    -- WIIFORM_BUILD_JOBS caps the compilers run at once, for hosts with little memory.
    import("package.tools.make").build(package, {}, {curdir = builddir, jobs = os.getenv("WIIFORM_BUILD_JOBS")})
    os.cp(path.join(builddir, "libwiiqt-nand.a"), package:installdir("lib"))
    os.cp("WiiQt/*.h", package:installdir("include"))
    os.cp("COPYING", package:installdir("share", "licenses", "wiiqt-nand", "COPYING"))
  end)
package_end()
