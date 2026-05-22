{
  "targets": [
    {
      "target_name": "tree_sitter_loom_binding",
      "include_dirs": [
        "<!(node -e \"require('node-addon-api').include_dir\")",
        "src"
      ],
      "sources": [
        "bindings/node/binding.cc",
        "src/parser.c",
        "src/scanner.c"
      ],
      "cflags_c": [
        "-std=c11"
      ]
    }
  ]
}
