# SPDX-License-Identifier: GPL-2.0-only
"""Extract the pinned Priiloader NAND app and definitions for release packaging."""

from hashlib import sha256
from io import BytesIO
import os
from pathlib import Path
import sys
from urllib.request import urlopen
from zipfile import ZipFile

RELEASE_URL = "https://github.com/DacoTaco/priiloader/releases/download/0.10.0/Priiloader_v0_10.zip"
ARCHIVE_LIMIT = 1024 * 1024
ARCHIVE_SHA256 = "312881836bc62ae55d496a9caef987d19493f635e26079bc60ecde08a2a31c1d"
APP_SHA256 = "f4508801cc4274bbcb74c07551d591486209c634fbf44f65765685fb19ec0157"
DEFINITIONS_SHA256 = "aea1a0009a327298f0608809c240aaf0ca5f5de3b12ef7ad761901dc4cb74ff4"

# Fixed layout of the compressed boot.dol in the verified release archive.
PACKED_OFFSET = 0x8E0
PACKED_SIZE = 0x85100
INSTALLER_SIZE = 0x11F600
EMBEDDED_APP_OFFSET = 0x4EA40
APP_SIZE = 747_968
END_MARKER = 256
MATCH_LENGTHS = 62
DISTANCE_BITS = (4, 6, 8, 10, 12, 14)
DISTANCE_BASES = (0, 16, 80, 336, 1360, 5456)


class AdaptiveHuffman:
    """The installer stub's adaptive tree, with literals, EOF and LZ matches."""

    FIRST_LEAF = END_MARKER + 1 + len(DISTANCE_BITS) * MATCH_LENGTHS
    NODE_COUNT = 2 * FIRST_LEAF
    ROOT = 1
    RESCALE_WEIGHT = 2000

    def __init__(self):
        self.left = [2 * node for node in range(self.FIRST_LEAF)]
        self.right = [node + 1 for node in self.left]
        self.parent = [node // 2 for node in range(self.NODE_COUNT)]
        self.weight = [0, 0] + [1] * (self.NODE_COUNT - 2)

    def sibling(self, node, parent):
        return self.right[parent] if self.left[parent] == node else self.left[parent]

    def replace_child(self, parent, previous, replacement):
        if self.left[parent] == previous:
            self.left[parent] = replacement
        else:
            self.right[parent] = replacement
        self.parent[replacement] = parent

    def update_weights(self, node, sibling):
        while True:
            parent = self.parent[node]
            self.weight[parent] = self.weight[node] + self.weight[sibling]
            if parent == self.ROOT:
                break
            node = parent
            sibling = self.sibling(node, self.parent[node])
        if self.weight[self.ROOT] == self.RESCALE_WEIGHT:
            for node in range(self.ROOT, self.NODE_COUNT):
                self.weight[node] //= 2

    def read(self, read_bit):
        node = self.ROOT
        while node < self.FIRST_LEAF:
            node = self.right[node] if read_bit() else self.left[node]
        symbol = node - self.FIRST_LEAF
        self.weight[node] += 1
        parent = self.parent[node]
        if parent != self.ROOT:
            self.update_weights(node, self.sibling(node, parent))
        while parent != self.ROOT:
            grandparent = self.parent[parent]
            uncle = self.sibling(parent, grandparent)
            if self.weight[node] > self.weight[uncle]:
                # Promote the heavier node and fix the weights of the branch it left.
                sibling = self.sibling(node, parent)
                self.replace_child(grandparent, uncle, node)
                self.replace_child(parent, node, uncle)
                node = uncle
                self.update_weights(node, sibling)
            node = self.parent[node]
            parent = self.parent[node]
        return symbol


def expand_installer(packed):
    tree = AdaptiveHuffman()
    bits = (byte >> shift & 1 for byte in packed for shift in range(7, -1, -1))
    read_bit = bits.__next__
    output = bytearray()
    try:
        while True:
            symbol = tree.read(read_bit)
            if symbol < END_MARKER:
                output.append(symbol)
            elif symbol == END_MARKER:
                return output
            else:
                # Each of the six ranges encodes match lengths 3 through 64.
                group, length_code = divmod(symbol - END_MARKER - 1, MATCH_LENGTHS)
                length = length_code + 3
                extra_distance = sum(read_bit() << i for i in range(DISTANCE_BITS[group]))
                # The distance includes the length, so the source precedes the match.
                distance = DISTANCE_BASES[group] + length + extra_distance
                if distance > len(output):
                    raise ValueError("Invalid backreference in the Priiloader installer")
                start = len(output) - distance
                output.extend(output[start:start + length])
            if len(output) > INSTALLER_SIZE:
                raise ValueError("Expanded Priiloader installer exceeds its pinned size")
    except StopIteration as exc:
        raise ValueError("Truncated compressed Priiloader installer") from exc


def check_digest(data, expected, label):
    if sha256(data).hexdigest() != expected:
        raise ValueError(f"{label} does not match the pinned SHA-256")


def prepare(output):
    archive_path = os.environ.get("WIIFORM_PRIILOADER_ARCHIVE")
    stream = Path(archive_path).open("rb") if archive_path else urlopen(RELEASE_URL, timeout=30)
    with stream:
        archive = stream.read(ARCHIVE_LIMIT + 1)
    if len(archive) > ARCHIVE_LIMIT:
        raise ValueError("Priiloader release archive exceeds its size limit")
    check_digest(archive, ARCHIVE_SHA256, "Priiloader release archive")

    with ZipFile(BytesIO(archive)) as release:
        dol = release.read("apps/priiloader/boot.dol")
        definitions = release.read("apps/priiloader/hacks_hash.ini")
        license_text = release.read("LICENSE")

    if len(dol) != PACKED_OFFSET + PACKED_SIZE:
        raise ValueError("Unexpected Priiloader boot.dol size")
    installer = expand_installer(dol[PACKED_OFFSET:])
    if len(installer) != INSTALLER_SIZE:
        raise ValueError("Unexpected expanded Priiloader installer size")
    app = installer[EMBEDDED_APP_OFFSET:EMBEDDED_APP_OFFSET + APP_SIZE]
    check_digest(app, APP_SHA256, "Priiloader app")
    check_digest(definitions, DEFINITIONS_SHA256, "Priiloader hack definitions")

    output.mkdir(parents=True, exist_ok=True)
    (output / "priiloader.app").write_bytes(app)
    (output / "hacks_hash.ini").write_bytes(definitions)
    (output / "LICENSE").write_bytes(license_text)


if __name__ == "__main__":
    prepare(Path(sys.argv[1]))
