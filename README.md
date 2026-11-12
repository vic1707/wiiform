# Wiiform

Wiiform builds and verifies Wii NAND images from console seeds. Its native
library provides the operations used by the command-line tools and the recipe
builder.

This repository starts from the behavior of wiicli-tools at commit `ab202ef`.
The migration uses host-side verification and synthetic tests. The original
project retains the historical GUI comparison and console validation records.
