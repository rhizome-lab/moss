// Example cobra CLI for testing help output parsing.
package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var (
	verbose bool
	config  string
	port    int
)

var rootCmd = &cobra.Command{
	Use:     "example",
	Short:   "An example CLI tool for testing",
	Version: "1.0.0",
}

var buildCmd = &cobra.Command{
	Use:   "build",
	Short: "Build the project",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Building...")
	},
}

var runCmd = &cobra.Command{
	Use:   "run [args...]",
	Short: "Run the project",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Running with args:", args)
	},
}

var cleanCmd = &cobra.Command{
	Use:   "clean",
	Short: "Clean build artifacts",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Cleaning...")
	},
}

func init() {
	rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "Enable verbose output")
	rootCmd.PersistentFlags().StringVarP(&config, "config", "c", "", "Config file path")
	rootCmd.PersistentFlags().IntVarP(&port, "port", "p", 8080, "Port number")

	buildCmd.Flags().BoolP("release", "r", false, "Build in release mode")
	buildCmd.Flags().StringP("target", "t", "", "Target directory")

	rootCmd.AddCommand(buildCmd)
	rootCmd.AddCommand(runCmd)
	rootCmd.AddCommand(cleanCmd)
}

func main() {
	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}
