// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <nandbin.h>
#include <sharedcontentmap.h>
#include <uidmap.h>
#include <tools.h>

bool check_title_metadata(const QByteArray& tmd, const QByteArray& ticket);
bool decrypt_title(NusJob& job);
bool initialize_system(NandBin& nand, const QByteArray& uid_data);
bool install_title(NandBin& nand, UIDmap& uid, SharedContentMap& shared, NusJob job);
