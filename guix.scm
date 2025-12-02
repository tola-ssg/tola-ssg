;;; GNU Guix package definition for tola
;;;
;;; This file can be used to build and install tola using GNU Guix:
;;;
;;;   guix build -f guix.scm
;;;   guix package -f guix.scm
;;;
;;; Or add it to your Guix configuration by referencing this channel.
;;;
;;; Note: The cargo-build-system will download and build dependencies
;;; automatically from crates.io. Ensure you have network access during build.

(use-modules (guix packages)
             (guix git-download)
             (guix build-system cargo)
             ((guix licenses) #:prefix license:)
             (gnu packages assembly)
             (gnu packages pkg-config))

(define-public tola
  (package
    (name "tola")
    (version "0.5.14")
    (source
     (origin
       (method git-fetch)
       (uri (git-reference
             (url "https://github.com/KawaYww/tola-ssg")
             (commit (string-append "v" version))))
       (file-name (git-file-name name version))
       ;; To get the correct hash, run:
       ;;   guix hash -x --serializer=nar <source-directory>
       ;; Or use a placeholder and let Guix tell you the correct hash
       (sha256
        (base32 "0000000000000000000000000000000000000000000000000000"))))
    (build-system cargo-build-system)
    (arguments
     '(#:install-source? #f))
    (native-inputs
     (list nasm pkg-config))
    (home-page "https://github.com/KawaYww/tola-ssg")
    (synopsis "Static site generator for Typst-based blogs")
    (description
     "Tola is a static site generator designed for Typst-based blogs.
It handles tedious tasks unrelated to Typst itself, including:
@itemize
@item Automatic extraction of embedded SVG images for smaller size and faster loading
@item Slugification of paths and fragments for posts
@item Watching changes and recompiling automatically
@item Local server for previewing the generated site
@item Built-in TailwindCSS support
@item Deployment to GitHub Pages
@item RSS 2.0 support
@end itemize")
    (license license:expat)))

tola
