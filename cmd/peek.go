package cmd

import (
	"os"
	"strconv"

	"github.com/spf13/cobra"
	"stash/store"
)

func newPeekCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "peek [n]",
		Short:         "Write the most recent entry's content to stdout without removing it",
		Args:          cobra.MaximumNArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			n := 1
			if len(args) == 1 {
				parsed, err := strconv.Atoi(args[0])
				if err != nil {
					return err
				}
				n = parsed
			}
			m, err := store.NthNewest(n)
			if err != nil {
				return err
			}
			return store.Cat(m.ID, os.Stdout)
		},
	}
}
