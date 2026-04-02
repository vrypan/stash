package cmd

import (
	"encoding/json"
	"fmt"
	"os"
	"sort"

	"github.com/spf13/cobra"
	"stash/store"
)

func newAttrCmd() *cobra.Command {
	var sep string
	var jsonOut bool

	cmd := &cobra.Command{
		Use:           "attr <id|n|@n>",
		Short:         "Show user metadata for an entry",
		Args:          cobra.ExactArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			id, err := store.Resolve(args[0])
			if err != nil {
				return err
			}
			m, err := store.GetMeta(id)
			if err != nil {
				return err
			}
			if jsonOut {
				enc := json.NewEncoder(os.Stdout)
				enc.SetIndent("", "  ")
				return enc.Encode(m)
			}
			return writeAttrLines(m, sep)
		},
	}
	cmd.Flags().StringVar(&sep, "separator", "\t", "Separator used between key and value")
	cmd.Flags().BoolVar(&jsonOut, "json", false, "Output attributes as JSON")
	return cmd
}

func writeAttrLines(m store.Meta, sep string) error {
	if sep == "" {
		sep = "\t"
	}
	lines := [][2]string{
		{"id", m.DisplayID()},
		{"ts", m.TS},
		{"hash", m.Hash},
		{"size", fmt.Sprintf("%d", m.Size)},
	}
	if m.Type != "" {
		lines = append(lines, [2]string{"type", m.Type})
	}
	if m.MIME != "" {
		lines = append(lines, [2]string{"mime", m.MIME})
	}
	if len(m.Attrs) > 0 {
		keys := make([]string, 0, len(m.Attrs))
		for k := range m.Attrs {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			lines = append(lines, [2]string{"meta." + k, m.Attrs[k]})
		}
	}
	for _, line := range lines {
		fmt.Fprintf(os.Stdout, "%s%s%s\n", line[0], sep, line[1])
	}
	return nil
}
