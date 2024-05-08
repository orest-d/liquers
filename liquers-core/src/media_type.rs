pub fn file_extension_to_media_type(extension: &str) -> &str {
    match extension {
        "7z" => "application/x-7z-compressed",
        "aiff" => "audio/aiff",
        "arrow" => "application/octet-stream",
        "avi" => "video/avi",
        "avro" => "application/avro",
        "b" => "application/octet-stream",
        "bash" => "application/x-sh",
        "bat" => "application/x-bat",
        "bin" => "application/octet-stream",
        "blend" => "application/octet-stream",
        "bmp" => "image/bmp",
        "bson" => "application/bson",
        "bz2" => "application/x-bzip2",
        "c" => "text/x-csrc",
        "cbor" => "application/cbor",
        "cmd" => "application/x-cmd",
        "cpp" => "text/x-c++src",
        "cs" => "text/x-csharp",
        "css" => "text/css",
        "csv" => "text/csv",
        "db" => "application/octet-stream",
        "eps" => "image/eps",
        "epub" => "application/epub+zip",
        "fbx" => "application/fbx",
        "feather" => "application/octet-stream",
        "flac" => "audio/flac",
        "flv" => "video/x-flv",
        "geojson" => "application/geo+json",
        "ggml" => "application/octet-stream",
        "gguf" => "application/octet-stream",
        "gif" => "image/gif",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "go" => "text/x-go",
        "gz" => "application/gzip",
        "h" => "text/x-c++hdr",
        "h5" => "application/x-hdf",
        "hdf5" => "application/x-hdf",
        "hpp" => "text/x-c++hdr",
        "hql" => "application/x-hive",
        "htm" => "text/html",
        "html" => "text/html",
        "ico" => "image/x-icon",
        "ipynb" => "application/x-ipynb+json",
        "java" => "text/x-java",
        "jpeg" => "image/jpeg",
        "jpg" => "image/jpeg",
        "js" => "text/javascript",
        "json" => "application/json",
        "jsonl" => "application/jsonlines",
        "less" => "text/x-less",
        "lz4" => "application/lz4",
        "md" => "text/markdown",
        "mid" => "audio/midi",
        "midi" => "audio/midi",
        "mkv" => "video/x-matroska",
        "mobi" => "application/x-mobipocket-ebook",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "msgpack" => "application/x-msgpack",
        "nc" => "application/x-netcdf",
        "npy" => "application/octet-stream",
        "npz" => "application/octet-stream",
        "obj" => "model/obj",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "ogg" => "audio/ogg",
        "onnx" => "application/onnx",
        "orc" => "application/octet-stream",
        "parquet" => "application/octet-stream",
        "pb" => "application/octet-stream",
        "pbf" => "application/octet-stream",
        "pcd" => "application/octet-stream",
        "pcx" => "image/x-pcx",
        "pdf" => "application/pdf",
        "php" => "text/x-php",
        "pickle" => "application/octet-stream",
        "pkl" => "application/octet-stream",
        "ply" => "text/plain",
        "pmml" => "application/xml",
        "png" => "image/png",
        "ppm" => "image/x-portable-pixmap",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "protobuf" => "application/protobuf",
        "ps" => "application/postscript",
        "psd" => "application/octet-stream",
        "pt" => "application/octet-stream",
        "py" => "text/x-python",
        "r" => "text/x-rsrc",
        "rar" => "application/x-rar-compressed",
        "rdata" => "application/octet-stream",
        "rs" => "text/x-rustsrc",
        "rtf" => "application/rtf",
        "sass" => "text/x-sass",
        "scala" => "text/x-scala",
        "scss" => "text/x-scss",
        "sh" => "application/x-sh",
        "sql" => "application/sql",
        "sqlite" => "application/octet-stream",
        "stl" => "model/stl",
        "styl" => "text/x-stylus",
        "svg" => "image/svg+xml",
        "tar" => "application/x-tar",
        "tbz" => "application/x-bzip2",
        "tfrecord" => "application/octet-stream",
        "tga" => "image/tga",
        "tgz" => "application/gzip",
        "thrift" => "application/vnd.apache.thrift.binary",
        "tiff" => "image/tiff",
        "tlz4" => "application/lz4",
        "toml" => "application/x-toml",
        "tsv" => "text/tab-separated-values",
        "txt" => "text/plain",
        "txz" => "application/x-xz",
        "tzst" => "application/zstd",
        "wasm" => "application/wasm",
        "wav" => "audio/wav",
        "webm" => "video/webm",
        "webp" => "image/webp",
        "wmf" => "image/wmf",
        "wmv" => "video/x-ms-wmv",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "xbm" => "image/x-xbitmap",
        "xhtml" => "application/xhtml+xml",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xml" => "application/xml",
        "xz" => "application/x-xz",
        "yaml" => "application/x-yaml",
        "yml" => "application/x-yaml",
        "zip" => "application/zip",
        "zst" => "application/zstd",
        _ => "application/octet-stream",
    }
}
