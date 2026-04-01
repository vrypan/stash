package cmd

import (
	"time"

	"github.com/fatih/color"
	"github.com/spf13/cobra"
	"stash/store"
)

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
			m, err := store.GetMeta(id)
			if err != nil {
				return err
			}
			s := store.Summary{Meta: m}
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
