package cmd

import (
	"fmt"
	"time"

	"github.com/fatih/color"
	"github.com/spf13/cobra"
	"stash/store"
)

func getInspectSummary(id string) (store.Summary, error) {
	var summary store.Summary
	found := false
	err := store.StreamSummaries(func(s store.Summary) (bool, error) {
		if s.ID != id {
			return true, nil
		}
		summary = s
		found = true
		return false, nil
	})
	if err != nil {
		return store.Summary{}, err
	}
	if found {
		return summary, nil
	}

	m, err := store.GetMeta(id)
	if err != nil {
		return store.Summary{}, err
	}
	return store.Summary{Meta: m}, nil
}

func newInspectCmd() *cobra.Command {
	var chars int
	var formatStr string
	var noColor bool

	cmd := &cobra.Command{
		Use:           "inspect <id|n|@n>",
		Short:         "Show full details for a single entry",
		Args:          cobra.ExactArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			if noColor {
				color.NoColor = true
			}

			id, err := resolveEntryRef(args)
			if err != nil {
				return err
			}
			s, err := getInspectSummary(id)
			if err != nil {
				return fmt.Errorf("inspect %s: %w", id, err)
			}
			if formatStr != "" {
				return logTemplate([]store.Summary{s}, time.Now(), chars, "absolute", formatStr)
			}
			return logLong([]store.Summary{s}, time.Now(), chars, "absolute", "full")
		},
	}
	cmd.Flags().IntVar(&chars, "chars", 80, "Preview character limit")
	cmd.Flags().StringVar(&formatStr, "format", "", "Go template for custom inspect output")
	cmd.Flags().BoolVar(&noColor, "no-color", false, "Disable color output")
	return cmd
}
