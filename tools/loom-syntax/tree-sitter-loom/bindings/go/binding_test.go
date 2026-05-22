package tree_sitter_loom_test

import (
	"testing"

	tree_sitter "github.com/smacker/go-tree-sitter"
	"github.com/tree-sitter/tree-sitter-loom"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_loom.Language())
	if language == nil {
		t.Errorf("Error loading Loom grammar")
	}
}
