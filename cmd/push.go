package cmd

import (
	"fmt"
	"os"
	"strings"

	"github.com/spf13/cobra"
	"stash/store"
)

func newPushCmd() *cobra.Command {
	var metaFlags []string

	cmd := &cobra.Command{
		Use:           "push",
		Short:         "Read stdin and create a new entry (default command)",
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(c *cobra.Command, _ []string) error {
			return runPushWithMeta(c, metaFlags)
		},
	}

	cmd.Flags().StringArrayVarP(&metaFlags, "meta", "m", nil, "Metadata key=value (repeatable)")
	return cmd
}

func runPush(c *cobra.Command, _ []string) error {
	return runPushWithMeta(c, nil)
}

func runPushWithMeta(_ *cobra.Command, metaFlags []string) error {
	stat, err := os.Stdin.Stat()
	if err != nil {
		return err
	}
	if stat.Mode()&os.ModeCharDevice != 0 {
		return fmt.Errorf("no stdin provided")
	}

	var attrs map[string]string
	if len(metaFlags) > 0 {
		attrs = make(map[string]string, len(metaFlags))
		for _, kv := range metaFlags {
			k, v, ok := strings.Cut(kv, "=")
			if !ok {
				return fmt.Errorf("invalid --meta value %q: expected key=value", kv)
			}
			attrs[k] = v
		}
	}

	id, err := store.Push(os.Stdin, attrs)
	if err != nil {
		return err
	}
	fmt.Println(strings.ToLower(id))
	return nil
}
