#!/usr/bin/env node
/**
 * Example yargs CLI for testing help output parsing.
 */

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

yargs(hideBin(process.argv))
  .scriptName('example')
  .usage('$0 <command> [options]')
  .option('verbose', {
    alias: 'v',
    type: 'boolean',
    description: 'Enable verbose output'
  })
  .option('config', {
    alias: 'c',
    type: 'string',
    description: 'Config file path'
  })
  .option('port', {
    alias: 'p',
    type: 'number',
    default: 8080,
    description: 'Port number'
  })
  .command('build', 'Build the project', (yargs) => {
    return yargs
      .option('release', {
        alias: 'r',
        type: 'boolean',
        description: 'Build in release mode'
      })
      .option('target', {
        alias: 't',
        type: 'string',
        description: 'Target directory'
      });
  })
  .command('run [args..]', 'Run the project', (yargs) => {
    return yargs.positional('args', {
      description: 'Arguments to pass',
      type: 'string'
    });
  })
  .command('clean', 'Clean build artifacts')
  .demandCommand(1)
  .help()
  .version('1.0.0')
  .parse();
