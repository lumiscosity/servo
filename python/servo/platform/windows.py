# Copyright 2023 The Servo Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

import os
import subprocess
import tempfile
from typing import Optional
import urllib.parse
import zipfile
import shutil

from servo import util

from .base import Base
from .build_target import BuildTarget

DEPS_URL = "https://github.com/servo/servo-build-deps/releases/download/msvc-deps"
DEPENDENCIES = {
    "moztools": "4.0",
}

GSTREAMER_URL = f"{DEPS_URL}/gstreamer-1.0-msvc-x86_64-1.22.8.msi"
GSTREAMER_DEVEL_URL = f"{DEPS_URL}/gstreamer-1.0-devel-msvc-x86_64-1.22.8.msi"
DEPENDENCIES_DIR = os.path.join(util.get_target_dir(), "dependencies")

WINGET_DEPENDENCIES = ["Kitware.CMake", "LLVM.LLVM", "Ninja-build.Ninja", "WiXToolset.WiXToolset"]


def get_dependency_dir(package: str) -> str:
    """Get the directory that a given Windows dependency should extract to."""
    return os.path.join(DEPENDENCIES_DIR, package, DEPENDENCIES[package])


def _winget_import(force: bool = False) -> None:
    try:
        # We install tools like LLVM / CMake, so we probably don't want to force-upgrade
        # a user installed version without good reason.
        cmd = ["winget", "install", "--interactive"]
        if force:
            cmd.append("--force")
        else:
            cmd.append("--no-upgrade")

        cmd.extend(WINGET_DEPENDENCIES)

        # The output will be printed to the terminal that `./mach bootstrap` is running in.
        subprocess.run(cmd, encoding="utf-8")
    except subprocess.CalledProcessError as e:
        print("Could not run winget.  Follow manual build setup instructions.")
        raise e


def _choco_install(force: bool = False) -> None:
    try:
        choco_config = os.path.join(util.SERVO_ROOT, "support", "windows", "chocolatey.config")

        # This is the format that PowerShell wants arguments passed to it.
        cmd_exe_args = f"'/K','choco','install','-y', '\"{choco_config}\"'"
        if force:
            cmd_exe_args += ",'-f'"

        print(cmd_exe_args)
        subprocess.check_output(
            ["powershell", "Start-Process", "-Wait", "-verb", "runAs", "cmd.exe", "-ArgumentList", f"@({cmd_exe_args})"]
        ).decode("utf-8")
    except subprocess.CalledProcessError as e:
        print("Could not run chocolatey.  Follow manual build setup instructions.")
        raise e


class Windows(Base):
    def __init__(self, triple: str) -> None:
        super().__init__(triple)
        self.is_windows = True

    def executable_suffix(self) -> str:
        return ".exe"

    @classmethod
    def download_and_extract_dependency(cls, zip_path: str, full_spec: str) -> None:
        if not os.path.isfile(zip_path):
            zip_url = f"{DEPS_URL}/{urllib.parse.quote(full_spec)}.zip"
            util.download_file(full_spec, zip_url, zip_path)

        zip_dir = os.path.dirname(zip_path)
        print(f"Extracting {full_spec} to {zip_dir}...", end="")
        try:
            util.extract(zip_path, zip_dir)
        except zipfile.BadZipfile:
            print(f"\nError: {full_spec}.zip is not a valid zip file, redownload...")
            os.remove(zip_path)
            cls.download_and_extract_dependency(zip_path, full_spec)
        else:
            print("done")

    def _platform_bootstrap(self, force: bool) -> bool:
        installed_something = self.passive_bootstrap()
        # If `winget` works well in practice, we could switch the default in the future.
        if shutil.which("choco") is not None:
            _choco_install(force)
        else:
            _winget_import()

        target = BuildTarget.from_triple(None)
        installed_something |= self._platform_bootstrap_gstreamer(target, force)
        return installed_something

    def passive_bootstrap(self) -> bool:
        """A bootstrap method that is called without explicitly invoking `./mach bootstrap`
        but that is executed in the process of other `./mach` commands. This should be
        as fast as possible."""
        to_install = [package for package in DEPENDENCIES if not os.path.isdir(get_dependency_dir(package))]
        if not to_install:
            return False

        print("Installing missing MSVC dependencies...")
        for package in to_install:
            full_spec = "{}-{}".format(package, DEPENDENCIES[package])

            package_dir = get_dependency_dir(package)
            parent_dir = os.path.dirname(package_dir)
            if not os.path.isdir(parent_dir):
                os.makedirs(parent_dir)

            self.download_and_extract_dependency(package_dir + ".zip", full_spec)
            os.rename(os.path.join(parent_dir, full_spec), package_dir)

        return True

    def gstreamer_root(self, target: BuildTarget) -> Optional[str]:
        build_target_triple = target.triple()
        gst_arch_names = {
            "x86_64": "X86_64",
            "x86": "X86",
            "aarch64": "ARM64",
        }
        gst_arch_name = gst_arch_names[build_target_triple.split("-")[0]]

        # The bootstraped version of GStreamer always takes precedance of the installed vesion.
        prepackaged_root = os.path.join(DEPENDENCIES_DIR, "gstreamer", "1.0", f"msvc_{gst_arch_name}")
        if os.path.exists(os.path.join(prepackaged_root, "bin", "ffi-7.dll")):
            return prepackaged_root

        # The installed version of GStreamer often sets an environment variable pointing to
        # the install location.
        root_from_env = os.environ.get(f"GSTREAMER_1_0_ROOT_MSVC_{gst_arch_name}")
        if root_from_env and os.path.exists(os.path.join(root_from_env, "bin", "ffi-7.dll")):
            return root_from_env

        # If all else fails, look for an installation in the default install directory.
        default_root = os.path.join("C:\\gstreamer\\1.0", f"msvc_{gst_arch_name}")
        if os.path.exists(os.path.join(default_root, "bin", "ffi-7.dll")):
            return default_root

        return None

    def is_gstreamer_installed(self, target: BuildTarget) -> bool:
        return self.gstreamer_root(target) is not None

    def _platform_bootstrap_gstreamer(self, target: BuildTarget, force: bool) -> bool:
        if not force and self.is_gstreamer_installed(target):
            return False

        if "x86_64" not in self.triple:
            print("Bootstrapping gstreamer not supported on non-x86-64 Windows. Please install manually")
            return False

        with tempfile.TemporaryDirectory() as temp_dir:
            libs_msi = os.path.join(temp_dir, GSTREAMER_URL.rsplit("/", maxsplit=1)[-1])
            devel_msi = os.path.join(temp_dir, GSTREAMER_DEVEL_URL.rsplit("/", maxsplit=1)[-1])

            util.download_file("GStreamer libraries", GSTREAMER_URL, libs_msi)
            util.download_file("GStreamer development support", GSTREAMER_DEVEL_URL, devel_msi)

            print(f"Installing GStreamer packages to {DEPENDENCIES_DIR}...")
            os.makedirs(DEPENDENCIES_DIR, exist_ok=True)

            for installer in [libs_msi, devel_msi]:
                arguments = [
                    "/a",
                    f'"{installer}"TARGETDIR="{DEPENDENCIES_DIR}"',  # Install destination
                    "/qn",  # Quiet mode
                ]
                quoted_arguments = ",".join((f"'{arg}'" for arg in arguments))
                subprocess.check_call(
                    [
                        "powershell",
                        "exit (Start-Process",
                        "-PassThru",
                        "-Wait",
                        "-verb",
                        "runAs",
                        "msiexec.exe",
                        "-ArgumentList",
                        f"@({quoted_arguments})",
                        ").ExitCode",
                    ]
                )

            assert self.is_gstreamer_installed(target)
            return True
