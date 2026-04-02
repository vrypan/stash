package cmd

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

func newMetadataCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "metadata <id> [set key=value ... | unset key ...]",
		Aliases:       []string{"meta"},
		Short:         "Show or update user metadata for an entry",
		Args:          cobra.MinimumNArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			id, err := store.Resolve(args[0])
			if err != nil {
				return err
			}

			if len(args) == 1 {
				return printMetadata(id)
			}

			switch args[1] {
			case "set":
				return runMetadataSet(id, args[2:])
			case "unset":
				return runMetadataUnset(id, args[2:])
			default:
				return fmt.Errorf("unknown metadata action %q", args[1])
			}
		},
	}
}

func loadAttrs(id string) (map[string]string, error) {
	m, err := store.GetMeta(id)
	if err != nil {
		return nil, err
	}
	return m.Attrs, nil
}

func writeAttrs(w io.Writer, attrs map[string]string, sep string, jsonOut bool) error {
	if jsonOut {
		enc := json.NewEncoder(w)
		enc.SetIndent("", "  ")
		return enc.Encode(attrs)
	}
	if sep == "" {
		sep = "\t"
	}
	if len(attrs) == 0 {
		return nil
	}
	keys := make([]string, 0, len(attrs))
	for k := range attrs {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		fmt.Fprintf(w, "%s%s%s\n", k, sep, attrs[k])
	}
	return nil
}

func printMetadata(id string) error {
	attrs, err := loadAttrs(id)
	if err != nil {
		return err
	}
	return writeAttrs(os.Stdout, attrs, "=", false)
}

func printAttrs(id, sep string, jsonOut bool) error {
	attrs, err := loadAttrs(id)
	if err != nil {
		return err
	}
	return writeAttrs(os.Stdout, attrs, sep, jsonOut)
}

func runMetadataSet(id string, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("metadata set requires at least one key=value pair")
	}
	attrs := make(map[string]string, len(args))
	for _, kv := range args {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			return fmt.Errorf("invalid metadata value %q: expected key=value", kv)
		}
		attrs[k] = v
	}
	return store.WithLock(func() error {
		return store.SetAttrs(id, attrs)
	})
}

func runMetadataUnset(id string, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("metadata unset requires at least one key")
	}
	keys := append([]string(nil), args...)
	sort.Strings(keys)
	return store.WithLock(func() error {
		return store.UnsetAttrs(id, keys)
	})
}
