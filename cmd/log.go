package cmd

import (
	"fmt"
	"os"
	"sort"
	"strings"
	"text/tabwriter"
	"time"

	"stash/store"

	"github.com/spf13/cobra"
)

func newLogCmd() *cobra.Command {
	var fullFlag bool
	var chars int
	var hashFlag bool
	var typeFlag bool
	var n int
	var reverse bool
	var long bool
	var dateMode string

	cmd := &cobra.Command{
		Use:           "log",
		Short:         "Show entry history with content preview",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			if dateMode != "absolute" && dateMode != "relative" {
				return fmt.Errorf("--date must be absolute or relative")
			}

			entries, err := store.List()
			if err != nil {
				return err
			}

			if reverse {
				for i, j := 0, len(entries)-1; i < j; i, j = i+1, j-1 {
					entries[i], entries[j] = entries[j], entries[i]
				}
			}
			if n > 0 && len(entries) > n {
				entries = entries[:n]
			}

			now := time.Now()

			if long {
				return logLong(entries, now, chars, dateMode)
			}
			return logCompact(entries, now, chars, fullFlag, hashFlag, typeFlag, dateMode)
		},
	}

	cmd.Flags().BoolVar(&fullFlag, "full", false, "Show full canonical ULIDs")
	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().BoolVar(&hashFlag, "hash", false, "Include hash prefix")
	cmd.Flags().BoolVar(&typeFlag, "type", false, "Include detected content type")
	cmd.Flags().IntVarP(&n, "number", "n", 0, "Limit number of entries shown (0 = all)")
	cmd.Flags().BoolVar(&reverse, "reverse", false, "Show oldest first")
	cmd.Flags().BoolVarP(&long, "long", "l", false, "Verbose block format")
	cmd.Flags().StringVar(&dateMode, "date", "relative", "Date format: relative or absolute")
	return cmd
}

func logCompact(entries []store.Meta, now time.Time, chars int, full, hash, typ bool, dateMode string) error {
	w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	for _, m := range entries {
		idStr := m.ShortID()
		if full {
			idStr = m.DisplayID()
		}
		tsStr := formatTS(parseTS(m.TS), now, dateMode)
		typeStr, preview, _ := store.SmartPreview(m.ID, chars)
		if a := fmtAttrs(m.Attrs); a != "" {
			preview = preview + "  " + a
		}

		switch {
		case hash && typ:
			fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\n", idStr, store.HumanSize(m.Size), tsStr, typeStr, m.Hash, preview)
		case hash:
			fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\n", idStr, store.HumanSize(m.Size), tsStr, m.Hash, preview)
		case typ:
			fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\n", idStr, store.HumanSize(m.Size), tsStr, typeStr, preview)
		default:
			fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", idStr, store.HumanSize(m.Size), tsStr, preview)
		}
	}
	return w.Flush()
}

// fmtAttrs formats a meta map as "[key=val  key=val]", sorted by key.
// Returns an empty string when attrs is nil or empty.
func fmtAttrs(attrs map[string]string) string {
	if len(attrs) == 0 {
		return ""
	}
	keys := make([]string, 0, len(attrs))
	for k := range attrs {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	pairs := make([]string, len(keys))
	for i, k := range keys {
		pairs[i] = k + "=" + attrs[k]
	}
	return "[" + strings.Join(pairs, "  ") + "]"
}

func logLong(entries []store.Meta, now time.Time, chars int, dateMode string) error {
	for i, m := range entries {
		if i > 0 {
			fmt.Println()
		}
		tsStr := formatTS(parseTS(m.TS), now, dateMode)
		typeStr, _, _ := store.SmartPreview(m.ID, chars)
		hashPrefix := m.Hash

		fmt.Printf("entry %s\n", m.DisplayID())
		fmt.Printf("Short: %s\n", m.ShortID())
		fmt.Printf("Date:  %s\n", tsStr)
		fmt.Printf("Size:  %s\n", store.HumanSize(m.Size))
		fmt.Printf("Type:  %s\n", typeStr)
		fmt.Printf("Hash:  %s\n", hashPrefix)
		if a := fmtAttrs(m.Attrs); a != "" {
			fmt.Printf("Meta:  %s\n", a)
		}

		lines, _ := store.LongPreview(m.ID, chars, 5)
		// Drop trailing blank lines.
		for len(lines) > 0 && lines[len(lines)-1] == "" {
			lines = lines[:len(lines)-1]
		}
		if len(lines) > 0 {
			fmt.Printf("\n    %s\n", lines[0])
			for _, line := range lines[1:] {
				fmt.Printf("    %s\n", line)
			}
		}
	}
	return nil
}
