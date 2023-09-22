#!/bin/sh
cd $(dirname $0)
bindgen \
--opaque-type "std.*" \
--allowlist-type "Sh.*" \
--allowlist-var "SH.*" \
--rustified-enum "Sh.*" \
-o bindings.rs \
bindings.hpp \
-- -I../../gfx/angle/checkout/include \
-I/usr/local/opt/llvm/include/c++/v1/ \
-std=c++14
