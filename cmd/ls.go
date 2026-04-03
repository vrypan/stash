package cmd

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"stash/store"

	"github.com/spf13/cobra"
)

func newLsCmd() *cobra.Command {
	var chars int
	var idMode string
	var metaFilters []string
	var dateMode string
	var sizeMode string
	var name bool
	var n int
	var preview bool
	var reverse bool
	var long bool

	cmd := &cobra.Command{
		Use:           "ls",
		Short:         "List stash entries",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(c *cobra.Command, args []string) error {
			if dateMode != "" && dateMode != "absolute" && dateMode != "relative" && dateMode != "ls" {
				return fmt.Errorf("--date must be absolute, relative, or ls")
			}
			if sizeMode != "" && sizeMode != "human" && sizeMode != "bytes" {
				return fmt.Errorf("--size must be human or bytes")
			}
			if idMode != "short" && idMode != "full" && idMode != "pos" {
				return fmt.Errorf("--id must be short, full, or pos")
			}
			if len(args) > 0 {
				return fmt.Errorf("unexpected arguments: %s", strings.Join(args, " "))
			}
			effectiveDateMode := dateMode
			effectiveSizeMode := sizeMode
			effectiveName := name
			if long {
				if !c.Flags().Changed("date") {
					effectiveDateMode = "absolute"
				}
				if !c.Flags().Changed("size") {
					effectiveSizeMode = "human"
				}
				if !c.Flags().Changed("name") {
					effectiveName = true
				}
			}
			effectiveChars := chars

			metaSel, err := parseMetaSelection(metaFilters)
			if err != nil {
				return err
			}
			entries, err := collectEntries(metaSel, reverse, n)
			if err != nil {
				return err
			}

			now := time.Now()
			if preview && !c.Flags().Changed("chars") {
				effectiveChars = autoLSPreviewChars(entries, now, idMode, effectiveDateMode, effectiveSizeMode, effectiveName, metaSel)
			}
			return renderLS(entries, now, idMode, effectiveDateMode, effectiveSizeMode, effectiveName, preview, effectiveChars, metaSel)
		},
	}

	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().StringVar(&idMode, "id", "short", "ID display: short, full, or pos")
	cmd.Flags().StringArrayVarP(&metaFilters, "meta", "m", nil, "Show metadata tags with @, or filter by tag name (repeatable)")
	cmd.Flags().StringVar(&dateMode, "date", "", "Include date column: absolute, relative, or ls")
	cmd.Flags().Lookup("date").NoOptDefVal = "absolute"
	cmd.Flags().StringVar(&sizeMode, "size", "", "Include size column: human or bytes")
	cmd.Flags().Lookup("size").NoOptDefVal = "human"
	cmd.Flags().BoolVar(&name, "name", false, "Include filename or full ULID column")
	cmd.Flags().IntVarP(&n, "number", "n", 0, "Limit number of entries shown (0 = all)")
	cmd.Flags().BoolVarP(&preview, "preview", "p", false, "Append compact preview text")
	cmd.Flags().BoolVarP(&reverse, "reverse", "r", false, "Show oldest first")
	cmd.Flags().BoolVarP(&long, "long", "l", false, "Alias for --date --size --name")
	return cmd
}

func lsDate(t, now time.Time) string {
	if t.Year() == now.Year() {
		return t.Local().Format("Jan _2 15:04")
	}
	return t.Local().Format("Jan _2  2006")
}

func lsID(m store.Meta, idx int, idMode string) string {
	switch idMode {
	case "full":
		return m.DisplayID()
	case "pos":
		return fmt.Sprintf("%d", idx+1)
	default:
		return m.ShortID()
	}
}

func lsName(m store.Meta) string {
	if name := strings.TrimSpace(m.Attrs["filename"]); name != "" {
		return name
	}
	return m.DisplayID()
}

func lsAllAttrs(attrs map[string]string) string {
	if len(attrs) == 0 {
		return ""
	}
	keys := make([]string, 0, len(attrs))
	for k := range attrs {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	parts := make([]string, 0, len(keys))
	for _, k := range keys {
		parts = append(parts, attrs[k])
	}
	return strings.Join(parts, "  ")
}

type lsRow struct {
	id, name, metaInline, size, date, preview string
	metaVals                                  []string
}

func formatLSSize(size int64, mode string) string {
	if mode == "bytes" {
		return fmt.Sprintf("%d", size)
	}
	return store.HumanSize(size)
}

func lsMetaColumns(entries []store.Meta, sel metaSelection) []string {
	if len(sel.tags) > 0 {
		return append([]string(nil), sel.tags...)
	}
	return nil
}

func autoLSPreviewChars(entries []store.Meta, now time.Time, idMode, dateMode, sizeMode string, name bool, metaSel metaSelection) int {
	width, ok := terminalWidth()
	if !ok {
		return 80
	}
	metaCols := lsMetaColumns(entries, metaSel)
	metaWidths := make([]int, len(metaCols))
	maxID, maxSize, maxDate, maxName, maxInlineMeta := 0, 0, 0, 0, 0
	for i, m := range entries {
		if id := lsID(m, i, idMode); len(id) > maxID {
			maxID = len(id)
		}
		if sizeMode != "" {
			if size := formatLSSize(m.Size, sizeMode); len(size) > maxSize {
				maxSize = len(size)
			}
		}
		if dateMode != "" {
			if date := formatLSDate(parseTS(m.TS), now, dateMode); len(date) > maxDate {
				maxDate = len(date)
			}
		}
		if name {
			if n := lsName(m); len(n) > maxName {
				maxName = len(n)
			}
		}
		if len(metaCols) > 0 {
			for idx, col := range metaCols {
				value := " "
				if v, ok := m.Attrs[col]; ok {
					value = v
				}
				if len(value) > metaWidths[idx] {
					metaWidths[idx] = len(value)
				}
			}
		} else if metaSel.showAll {
			if inline := lsAllAttrs(m.Attrs); len(inline) > maxInlineMeta {
				maxInlineMeta = len(inline)
			}
		}
	}
	fixed := maxID
	if sizeMode != "" {
		fixed += 2 + maxSize
	}
	if dateMode != "" {
		fixed += 2 + maxDate
	}
	if name {
		fixed += 2 + maxName
	}
	for _, w := range metaWidths {
		fixed += 2 + w
	}
	if maxInlineMeta > 0 {
		fixed += 2 + maxInlineMeta
	}
	chars := width - fixed - 2
	if chars < 20 {
		return 20
	}
	return chars
}

func buildLSRows(entries []store.Meta, now time.Time, dateMode, sizeMode, idMode string, name, preview bool, chars int, metaSel metaSelection) []lsRow {
	metaCols := lsMetaColumns(entries, metaSel)
	rows := make([]lsRow, len(entries))
	for i, m := range entries {
		r := lsRow{
			id: lsID(m, i, idMode),
		}
		if len(metaCols) > 0 {
			r.metaVals = make([]string, len(metaCols))
			for idx, col := range metaCols {
				r.metaVals[idx] = " "
				if v, ok := m.Attrs[col]; ok {
					r.metaVals[idx] = v
				}
			}
		} else if metaSel.showAll {
			r.metaInline = lsAllAttrs(m.Attrs)
		}
		if name {
			r.name = lsName(m)
		}
		if sizeMode != "" {
			r.size = formatLSSize(m.Size, sizeMode)
		}
		if dateMode != "" {
			r.date = formatLSDate(parseTS(m.TS), now, dateMode)
		}
		if preview && m.Preview != "" {
			r.preview = previewSnippet(m.Preview, chars)
		}
		rows[i] = r
	}
	return rows
}

func renderLS(entries []store.Meta, now time.Time, idMode, dateMode, sizeMode string, name, preview bool, chars int, metaSel metaSelection) error {
	showMeta := metaSel.showAll || len(metaSel.tags) > 0
	if dateMode == "" && sizeMode == "" && !name && !preview && !showMeta {
		for i, m := range entries {
			fmt.Println(lsID(m, i, idMode))
		}
		return nil
	}

	rows := buildLSRows(entries, now, dateMode, sizeMode, idMode, name, preview, chars, metaSel)
	metaCols := lsMetaColumns(entries, metaSel)
	if dateMode == "ls" {
		for i, m := range entries {
			rows[i].date = lsDate(parseTS(m.TS), now)
		}
	}

	maxID, maxName, maxSize, maxDate, maxInlineMeta := 0, 0, 0, 0, 0
	metaWidths := make([]int, len(metaCols))
	for _, r := range rows {
		if len(r.id) > maxID {
			maxID = len(r.id)
		}
		if len(r.name) > maxName {
			maxName = len(r.name)
		}
		if len(r.size) > maxSize {
			maxSize = len(r.size)
		}
		if len(r.date) > maxDate {
			maxDate = len(r.date)
		}
		if len(r.metaInline) > maxInlineMeta {
			maxInlineMeta = len(r.metaInline)
		}
		for idx, v := range r.metaVals {
			if len(v) > metaWidths[idx] {
				metaWidths[idx] = len(v)
			}
		}
	}

	for _, r := range rows {
		idCol := clrID(fmt.Sprintf("%-*s", maxID, r.id))
		parts := []string{idCol}
		if r.size != "" {
			parts = append(parts, fmt.Sprintf("%*s", maxSize, r.size))
		}
		if r.date != "" {
			parts = append(parts, fmt.Sprintf("%*s", maxDate, r.date))
		}
		if r.name != "" {
			nameCol := r.name
			if nameCol != r.id {
				nameCol = clrFile(fmt.Sprintf("%-*s", maxName, r.name))
			} else {
				nameCol = fmt.Sprintf("%-*s", maxName, r.name)
			}
			parts = append(parts, nameCol)
		}
		for idx, v := range r.metaVals {
			parts = append(parts, clrAttrs(fmt.Sprintf("%-*s", metaWidths[idx], v)))
		}
		if r.metaInline != "" {
			parts = append(parts, clrAttrs(fmt.Sprintf("%-*s", maxInlineMeta, r.metaInline)))
		}
		if r.preview != "" {
			parts = append(parts, r.preview)
		}
		line := strings.Join(parts, "  ")
		if width, ok := terminalWidth(); ok {
			line = trimANSIToWidth(line, width)
		}
		fmt.Println(line)
	}
	return nil
}
