#!/usr/bin/env node
/**
 * Example commander CLI for testing help output parsing.
 */

import { Command } from 'commander';

const program = new Command();

program
  .name('example')
  .description('An example CLI tool for testing')
  .version('1.0.0')
  .option('-v, --verbose', 'Enable verbose output')
  .option('-c, --config <FILE>', 'Config file path')
  .option('-p, --port <PORT>', 'Port number', '8080');

program
  .command('build')
  .description('Build the project')
  .option('-r, --release', 'Build in release mode')
  .option('-t, --target <DIR>', 'Target directory')
  .action(() => {
    console.log('Building...');
  });

program
  .command('run [args...]')
  .description('Run the project')
  .action((args) => {
    console.log('Running with args:', args);
  });

program
  .command('clean')
  .description('Clean build artifacts')
  .action(() => {
    console.log('Cleaning...');
  });

program.parse();
