(use-modules (guix packages)
             (guix search-paths)
             (gnu packages rust)
             (gnu packages commencement)
             (gnu packages base)
             (gnu packages pkg-config)
             (gnu packages sqlite)
             (gnu packages tls))

(define gcc-toolchain-with-cc
  (package
    (inherit gcc-toolchain)
    (native-search-paths
     (cons (search-path-specification
            (variable "CC")
            (files '("bin/gcc"))
            (file-type 'regular)
            (separator #f))
           (package-native-search-paths gcc-toolchain)))))

(packages->manifest
 (list rust-1.88
       (list rust-1.88 "cargo")
       gcc-toolchain-with-cc
       pkg-config
       sqlite
       openssl))
