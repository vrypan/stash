package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"stash/store"
)

func newPushCmd() *cobra.Command {
	return &cobra.Command{
		Use:           "push",
		Short:         "Read stdin and create a new entry (default command)",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE:          runPush,
	}
}

func runPush(_ *cobra.Command, _ []string) error {
	stat, err := os.Stdin.Stat()
	if err != nil {
		return err
	}
	if stat.Mode()&os.ModeCharDevice != 0 {
		return fmt.Errorf("no stdin provided")
	}

	id, err := store.Push(os.Stdin)
	if err != nil {
		return err
	}
	fmt.Println(id)
	return nil
}
