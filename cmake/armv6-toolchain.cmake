# Raspberry Pi Zero (armv6, armhf) クロスコンパイル用 CMake ツールチェーン
#
# 使い方:
#   cmake -DCMAKE_TOOLCHAIN_FILE=cmake/armv6-toolchain.cmake \
#         -DCMAKE_BUILD_TYPE=Release -DBUILD_STATIC_BINARY=ON \
#         -DHAVE_STDBOOL_H=1 -DHAVE_FSEEKO=1 -DHAVE_FTELLO=1 \
#         -DCMAKE_WORDS_BIGENDIAN=0 -DCMAKE_EXE_LINKER_FLAGS="-static" -B build-armv6
#
# 注意: Ubuntu の arm-linux-gnueabihf の libc は armv7 向けにビルドされているため、
# -static でリンクしても armv7 命令が混入し、Pi Zero (armv6) で動かない可能性がある。
# 生成物は実機で動作確認すること。

set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR arm)

set(CMAKE_C_COMPILER arm-linux-gnueabihf-gcc)

# armv6 (arm1176jzf-s) のハードフロート ABI
set(CMAKE_C_FLAGS_INIT "-march=armv6 -mfpu=vfp -mfloat-abi=hard")

# configure 時のチェック（try_compile）でリンクを行わない
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
