#!/usr/bin/env node

/* eslint-disable no-process-exit */
import { Command } from 'commander'
import {
  configureLogger,
  parsePubkey,
  parseWallet,
} from '@marinade.finance/cli-common'
import { NullWallet } from '@marinade.finance/cli-common/src/wallet'
import { Wallet as WalletInterface } from '@marinade.finance/web3js-common'
import { installCommands } from './commands'
import { Logger } from 'pino'
import { setValidatorBondsCliContext } from './context'
import { VALIDATOR_BONDS_PROGRAM_ID } from '@marinade.finance/validator-bonds-sdk'

const DEFAULT_KEYPAIR_PATH = '~/.config/solana/id.json'
export const logger: Logger = configureLogger()
const program = new Command()

program
  .version('1.1.6')
  .allowExcessArguments(false)
  .option(
    '-u, --cluster <cluster>',
    'solana cluster URL, accepts shortcuts (d/devnet, m/mainnet)',
    'http://127.0.0.1:8899'
  )
  .option('-c <cluster>', 'alias for "-u, --cluster"')
  .option('--commitment <commitment>', 'Commitment', 'confirmed')
  .option(
    '-k, --keypair <keypair-or-ledger>',
    'Wallet keypair (path or ledger url in format usb://ledger/[<pubkey>][?key=<derivedPath>]). ' +
      'Wallet keypair is used to pay for the transaction fees and as default value for signers. ' +
      `(default: ${DEFAULT_KEYPAIR_PATH})`
  )
  .option(
    '--program-id <pubkey>',
    `Program id of validator bonds contract (default: ${VALIDATOR_BONDS_PROGRAM_ID})`,
    parsePubkey
  )
  .option('-s, --simulate', 'Simulate', false)
  .option(
    '-p, --print-only',
    'Print only mode, no execution, instructions are printed in base64 to output. ' +
      'This can be used for placing the admin commands to SPL Governance UI by hand.',
    false
  )
  .option(
    '--skip-preflight',
    'transaction execution flag "skip-preflight", see https://solanacookbook.com/guides/retrying-transactions.html#the-cost-of-skipping-preflight',
    false
  )
  .option(
    '-d, --debug',
    'printing more detailed information of the CLI execution',
    false
  )
  .option('-v, --verbose', 'alias for --debug', false)
  .hook('preAction', async (command: Command, action: Command) => {
    if (command.opts().debug || command.opts().verbose) {
      logger.level = 'debug'
    }
    const printOnly = Boolean(command.opts().printOnly)

    const walletInterface = await parseWalletFromOpts(
      command.opts().keypair,
      printOnly,
      command.args,
      logger
    )

    setValidatorBondsCliContext({
      cluster: command.opts().cluster as string,
      wallet: walletInterface,
      programId: await command.opts().programId,
      simulate: Boolean(command.opts().simulate),
      printOnly,
      skipPreflight: Boolean(command.opts().skipPreflight),
      commitment: command.opts().commitment,
      logger,
      command: action.name(),
    })
  })

/**
 * --keypair (considered as 'wallet') could be defined or undefined (and default is on parsing).
 * For 'show*' command we don't need a working wallet, so we can use NullWallet.
 * For '--print-only' we don't need a working wallet, so we can use NullWallet.
 * For other commands we need a working wallet, when cannot be parsed then Error.
 */
async function parseWalletFromOpts(
  keypairArg: string,
  printOnly: boolean,
  commandArgs: string[],
  logger: Logger
): Promise<WalletInterface> {
  const wallet = keypairArg
  let walletInterface: WalletInterface
  try {
    walletInterface = wallet
      ? await parseWallet(wallet, logger)
      : await parseWallet(DEFAULT_KEYPAIR_PATH, logger)
  } catch (err) {
    if (
      commandArgs.find(arg => arg.includes('show-')) !== undefined ||
      printOnly
    ) {
      // when working with show command it does not matter to use NullWallet
      // for other instructions it could matter as the transaction fees cannot be paid by NullWallet
      // still using NullWallet is ok when one generates only --print-only
      logger.debug(
        `Cannot load --keypair wallet '${
          wallet || DEFAULT_KEYPAIR_PATH
        }' but it's show or --print-only command, using NullWallet`
      )
      walletInterface = new NullWallet()
    } else {
      const definedMsg =
        wallet !== undefined
          ? `--keypair wallet '${wallet}'`
          : `default keypair path ${DEFAULT_KEYPAIR_PATH}`
      logger.error(`Failed to use ${definedMsg}, exiting.`)
      throw err
    }
  }
  return walletInterface
}

installCommands(program)

program.parseAsync(process.argv).then(
  () => {
    process.exit()
  },
  (err: unknown) => {
    logger.error({ command: 'failed', err, args: process.argv })
    console.error(err)
    process.exit(200)
  }
)
