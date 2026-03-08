(use-modules (gnu packages commencement)
             (gnu packages pkg-config)
             (gnu packages rust))

(packages->manifest
 (list rust-1.88
       (list rust-1.88 "cargo")
       gcc-toolchain
       pkg-config))
