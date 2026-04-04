package cmd

import (
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"sort"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

var writableAttrKeyRe = regexp.MustCompile(`^[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*$`)

func newAttrCmd() *cobra.Command {
	var sep string
	var jsonOut bool
	var withPreview bool

	cmd := &cobra.Command{
		Use:           "attr <id|n|@n> [key | set key=value ... | unset key ...]",
		Short:         "Show or update entry attributes",
		Args:          cobra.MinimumNArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			id, err := store.Resolve(args[0])
			if err != nil {
				return err
			}
			m, preview, err := loadAttrData(id)
			if err != nil {
				return err
			}
			if len(args) > 1 {
				switch args[1] {
				case "set":
					return runAttrSet(id, args[2:])
				case "unset":
					return runAttrUnset(id, args[2:])
				default:
					if len(args) != 2 {
						return fmt.Errorf("attr get accepts exactly one key")
					}
					return printAttrValue(m, preview, args[1], sep, jsonOut)
				}
			}
			if jsonOut {
				enc := json.NewEncoder(os.Stdout)
				enc.SetIndent("", "  ")
				if withPreview && strings.TrimSpace(preview) != "" {
					return enc.Encode(struct {
						store.Meta
						Preview string `json:"preview,omitempty"`
					}{Meta: m, Preview: preview})
				}
				return enc.Encode(m)
			}
			return writeAttrLines(m, sep, withPreview, preview)
		},
	}
	cmd.Flags().StringVar(&sep, "separator", "\t", "Separator used between key and value")
	cmd.Flags().BoolVar(&jsonOut, "json", false, "Output attributes as JSON")
	cmd.Flags().BoolVarP(&withPreview, "preview", "p", false, "Include preview pseudo-property when available")
	return cmd
}

func loadAttrData(id string) (store.Meta, string, error) {
	m, err := store.GetMeta(id)
	if err != nil {
		return store.Meta{}, "", err
	}
	return m, m.Preview, nil
}

func attrValue(m store.Meta, preview, key string) (string, bool) {
	switch key {
	case "id":
		return m.DisplayID(), true
	case "ts":
		return m.TS, true
	case "size":
		return fmt.Sprintf("%d", m.Size), true
	case "preview":
		if strings.TrimSpace(preview) == "" {
			return "", false
		}
		return preview, true
	default:
		v, ok := m.Attrs[key]
		return v, ok
	}
}

func printAttrValue(m store.Meta, preview, key, sep string, jsonOut bool) error {
	v, ok := attrValue(m, preview, key)
	if !ok {
		return &store.ErrNotFound{Input: key}
	}
	if jsonOut {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		return enc.Encode(map[string]string{key: v})
	}
	fmt.Fprintln(os.Stdout, v)
	return nil
}

func isWritableAttrKey(key string) bool {
	switch key {
	case "id", "ts", "size", "preview":
		return false
	default:
		return writableAttrKeyRe.MatchString(key)
	}
}

func runAttrSet(id string, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("attr set requires at least one key=value pair")
	}
	metaArgs := make([]string, 0, len(args))
	for _, kv := range args {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			return fmt.Errorf("invalid attr value %q: expected key=value", kv)
		}
		k = strings.TrimSpace(k)
		if k == "" {
			return fmt.Errorf("invalid attr value %q: expected key=value", kv)
		}
		if !isWritableAttrKey(k) {
			return fmt.Errorf("only metadata keys are writable: %q", k)
		}
		metaArgs = append(metaArgs, k+"="+v)
	}
	return runMetadataSet(id, metaArgs)
}

func runAttrUnset(id string, args []string) error {
	if len(args) == 0 {
		return fmt.Errorf("attr unset requires at least one key")
	}
	keys := make([]string, 0, len(args))
	for _, k := range args {
		k = strings.TrimSpace(k)
		if k == "" {
			return fmt.Errorf("attr unset requires at least one key")
		}
		if !isWritableAttrKey(k) {
			return fmt.Errorf("only metadata keys are writable: %q", k)
		}
		keys = append(keys, k)
	}
	return runMetadataUnset(id, keys)
}

func writeAttrLines(m store.Meta, sep string, withPreview bool, preview string) error {
	if sep == "" {
		sep = "\t"
	}
	lines := [][2]string{
		{"id", m.DisplayID()},
		{"ts", m.TS},
		{"size", fmt.Sprintf("%d", m.Size)},
	}
	if len(m.Attrs) > 0 {
		keys := make([]string, 0, len(m.Attrs))
		for k := range m.Attrs {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			lines = append(lines, [2]string{k, m.Attrs[k]})
		}
	}
	if withPreview && strings.TrimSpace(preview) != "" {
		lines = append(lines, [2]string{"preview", preview})
	}
	for _, line := range lines {
		fmt.Fprintf(os.Stdout, "%s%s%s\n", line[0], sep, line[1])
	}
	return nil
}
