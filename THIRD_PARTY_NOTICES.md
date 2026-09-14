# Third-party notices

MiaoZip's RAR extraction uses the `unrar` and `unrar_sys` Rust crates, which link
the UnRAR decompression library. UnRAR is used only to read and extract RAR
archives; MiaoZip does not implement RAR creation.

The following notice is reproduced from the UnRAR license supplied with
`unrar_sys`:

> UnRAR source code may be used in any software to handle
> RAR archives without limitations free of charge, but cannot be
> used to develop RAR (WinRAR) compatible archiver and to
> re-create RAR compression algorithm, which is proprietary.
> Distribution of modified UnRAR source code in separate form
> or as a part of other software is permitted, provided that
> full text of this paragraph, starting from "UnRAR source code"
> words, is included in license, or in documentation if license
> is not available, and in source code comments of resulting package.

Copyright in RAR and UnRAR belongs to Alexander Roshal. The full UnRAR license
is included in the `unrar_sys` source package under `vendor/unrar/license.txt`.
