(use-modules (guix packages)
             (guix search-paths)
             (gnu packages commencement)
             (gnu packages base)
             (gnu packages pkg-config)
             (gnu packages sqlite)
             (gnu packages tls))

;; No rust here on purpose: sqlx 0.9 needs 1.94 and Guix currently tops out at
;; 1.93, so the toolchain comes from rustup. This manifest supplies everything
;; else the build links against.
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
 (list gcc-toolchain-with-cc
       pkg-config
       sqlite
       openssl))
