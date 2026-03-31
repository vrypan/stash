package cmd

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"stash/store"

	"github.com/fatih/color"
	"github.com/spf13/cobra"
)

var (
	clrID     = color.New(color.FgYellow, color.Bold).SprintFunc()
	clrTS     = color.New(color.FgCyan).SprintFunc()
	clrSize   = color.New(color.FgBlue).SprintFunc()
	clrHash   = color.New(color.Faint).SprintFunc()
	clrLabel  = color.New(color.Bold).SprintFunc()
	clrAttrs  = color.New(color.FgYellow).SprintFunc()
)

func typeColor(t string) color.Attribute {
	switch t {
	case "text":
		return color.FgGreen
	case "json":
		return color.FgCyan
	case "empty":
		return color.Faint
	default: // binary, gzip, zstd, zip, png, jpeg, pdf, gif, …
		return color.FgRed
	}
}

// clrType returns "[typeStr]" with color — for compact format.
func clrType(t string) string {
	return color.New(typeColor(t)).Sprint("[" + t + "]")
}

// clrTypeBare returns typeStr with color, no brackets — for long format.
func clrTypeBare(t string) string {
	return color.New(typeColor(t)).Sprint(t)
}

func newLogCmd() *cobra.Command {
	var fullFlag bool
	var chars int
	var hashFlag bool
	var n int
	var reverse bool
	var long bool
	var dateMode string
	var noColor bool

	cmd := &cobra.Command{
		Use:           "log",
		Short:         "Show entry history with content preview",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			if noColor {
				color.NoColor = true
			}
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
			return logCompact(entries, now, chars, fullFlag, hashFlag, dateMode)
		},
	}

	cmd.Flags().BoolVar(&fullFlag, "full", false, "Show full canonical ULIDs")
	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().BoolVar(&hashFlag, "hash", false, "Include hash")
	cmd.Flags().IntVarP(&n, "number", "n", 0, "Limit number of entries shown (0 = all)")
	cmd.Flags().BoolVar(&reverse, "reverse", false, "Show oldest first")
	cmd.Flags().BoolVarP(&long, "long", "l", false, "Verbose block format")
	cmd.Flags().StringVar(&dateMode, "date", "relative", "Date format: relative or absolute")
	cmd.Flags().BoolVar(&noColor, "no-color", false, "Disable color output")
	return cmd
}

func logCompact(entries []store.Meta, now time.Time, chars int, full, hash bool, dateMode string) error {
	type row struct {
		id, ts, size, hash, typeStr, attrsStr, preview string
		truncated                                       bool
	}
	rows := make([]row, len(entries))
	maxID, maxTS, maxSize, maxHash := 0, 0, 0, 0

	for i, m := range entries {
		idStr := m.ShortID()
		if full {
			idStr = m.DisplayID()
		}
		tsStr := formatTS(parseTS(m.TS), now, dateMode)
		typeStr, preview, _ := store.SmartPreview(m.ID, chars)
		sizeStr := store.HumanSize(m.Size)

		rows[i] = row{
			id:        idStr,
			ts:        tsStr,
			size:      sizeStr,
			hash:      m.Hash,
			typeStr:   typeStr,
			attrsStr:  fmtAttrs(m.Attrs),
			preview:   preview,
			truncated: (typeStr == "text" || typeStr == "json") && m.Size > int64(chars),
		}
		if len(idStr) > maxID {
			maxID = len(idStr)
		}
		if len(tsStr) > maxTS {
			maxTS = len(tsStr)
		}
		if len(sizeStr) > maxSize {
			maxSize = len(sizeStr)
		}
		if hash && len(m.Hash) > maxHash {
			maxHash = len(m.Hash)
		}
	}

	for _, r := range rows {
		// Pad plain strings first, then colorize — ANSI codes add invisible
		// bytes that would confuse any width-based padding done afterwards.
		idCol := clrID(fmt.Sprintf("%-*s", maxID, r.id))
		tsCol := clrTS(fmt.Sprintf("%-*s", maxTS, r.ts))
		sizeCol := clrSize(fmt.Sprintf("%-*s", maxSize, r.size))

		var parts []string
		parts = append(parts, clrType(r.typeStr))
		if r.attrsStr != "" {
			parts = append(parts, clrAttrs(r.attrsStr))
		}
		if (r.typeStr == "text" || r.typeStr == "json") && r.preview != "" {
			p := r.preview
			if r.truncated {
				p += "..."
			}
			parts = append(parts, p)
		}
		contentCol := strings.Join(parts, " ")

		if hash {
			hashCol := clrHash(fmt.Sprintf("%-*s", maxHash, r.hash))
			fmt.Printf("%s  %s  %s  %s  %s\n", idCol, tsCol, sizeCol, hashCol, contentCol)
		} else {
			fmt.Printf("%s  %s  %s  %s\n", idCol, tsCol, sizeCol, contentCol)
		}
	}
	return nil
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

		fmt.Printf("entry %s\n", clrID(m.DisplayID()))
		fmt.Printf("%s%s\n", clrLabel("Short: "), m.ShortID())
		fmt.Printf("%s%s\n", clrLabel("Date:  "), tsStr)
		fmt.Printf("%s%s\n", clrLabel("Size:  "), clrSize(store.HumanSize(m.Size)))
		fmt.Printf("%s%s\n", clrLabel("Type:  "), clrTypeBare(typeStr))
		fmt.Printf("%s%s\n", clrLabel("Hash:  "), clrHash(m.Hash))
		if a := fmtAttrs(m.Attrs); a != "" {
			fmt.Printf("%s%s\n", clrLabel("Meta:  "), clrAttrs(a))
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
