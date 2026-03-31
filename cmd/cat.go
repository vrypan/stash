package cmd

import (
	"os"

	"github.com/spf13/cobra"
	"stash/store"
)

func newCatCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "cat <id>",
		Short:         "Write the referenced entry's raw bytes to stdout",
		Args:          cobra.ExactArgs(1),
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(_ *cobra.Command, args []string) error {
			id, err := store.Resolve(args[0])
			if err != nil {
				return err
			}
			return store.Cat(id, os.Stdout)
		},
	}
}
