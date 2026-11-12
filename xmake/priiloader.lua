-- Build the pinned Priiloader installer app with the user's existing
-- devkitPPC/libogc installation. This is a task, not a default target, so it
-- does not add homebrew toolchain requirements to host builds.
task("build-priiloader")
    set_menu {
        usage = "xmake build-priiloader",
        description = "Build the pinned Priiloader installer app using devkitPPC",
        options = {}
    }

    on_run(function ()
        local project = os.projectdir()
        local source = path.join(project, "vendor", "priiloader")
        local makefile = path.join(source, "Makefile")
        local devkitppc = os.getenv("DEVKITPPC")
        local devkitpro = os.getenv("DEVKITPRO")

        if not os.isfile(makefile) then
            raise("Priiloader source is missing. Initialize the vendor/priiloader submodule first.")
        end
        if not devkitppc or devkitppc == "" then
            raise("DEVKITPPC is not set. Install devkitPPC and export DEVKITPPC to its directory.")
        end
        if not os.isfile(path.join(devkitppc, "wii_rules")) then
            raise("DEVKITPPC does not contain wii_rules: " .. devkitppc)
        end
        if not devkitpro or devkitpro == "" then
            raise("DEVKITPRO is not set. Priiloader also requires libogc from devkitPro.")
        end
        if not os.isdir(path.join(devkitpro, "libogc")) then
            raise("libogc was not found under DEVKITPRO: " .. devkitpro)
        end

        local revision = os.iorunv("git", {"-C", source, "rev-parse", "HEAD"}):trim()
        if revision ~= "2b70b3693e9dc3e144db1a73224921d405081663" then
            raise("Priiloader submodule must be at the pinned 0.10.0 commit")
        end
        if os.iorunv("git", {"-C", source, "status", "--porcelain"}):trim() ~= "" then
            raise("Priiloader source has local changes")
        end
        revision = revision:sub(1, 7)
        local version = "0.10.0-0-g" .. revision

        -- Upstream generates build outputs inside its source tree.
        local stage = path.join(project, "build", "priiloader-src")
        os.tryrm(stage)
        os.cp(source, stage)

        os.vrunv("make", {
            "-C", stage,
            "NUMBER=" .. revision,
            "NUMBERSTR=" .. version,
            "GITTAG=" .. version,
            "WIILOAD="
        }, {curdir = project})

        local app = path.join(stage, "src", "Installer", "data", "priiloader.app")
        if not os.isfile(app) then
            raise("Priiloader build completed without producing " .. app)
        end
        print("Priiloader installer app: " .. app)
    end)
task_end()
