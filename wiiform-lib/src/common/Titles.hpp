// SPDX-License-Identifier: GPL-2.0-only
#pragma once

#include <nandbin.h>
#include <sharedcontentmap.h>
#include <uidmap.h>
#include <tools.h>
#include <TitleValidation.hpp>

bool initialize_system(NandBin& nand, const QByteArray& uid_data);
bool install_title(NandBin& nand, UIDmap& uid, SharedContentMap& shared, ValidatedTitleJob job);
