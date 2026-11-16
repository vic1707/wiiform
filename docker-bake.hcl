# `docker buildx bake` builds the three command images from the Containerfile.
variable "REGISTRY" { default = "ghcr.io/vic1707" }
variable "TAG" { default = "dev" }

group "default" { targets = ["wiiform", "wii-seed", "wii-nand"] }

target "wiiform" {
  dockerfile = "Containerfile"
  target     = "wiiform"
  tags       = ["${REGISTRY}/wiiform:${TAG}"]
}

target "wii-seed" {
  inherits = ["wiiform"]
  target   = "wii-seed"
  tags     = ["${REGISTRY}/wii-seed:${TAG}"]
}

target "wii-nand" {
  inherits = ["wiiform"]
  target   = "wii-nand"
  tags     = ["${REGISTRY}/wii-nand:${TAG}"]
}
