import {
  parsePubkey,
  parsePubkeyOrPubkeyFromWallet,
  parseWalletOrPubkey,
} from '@marinade.finance/cli-common'
import { Command } from 'commander'
import { setProgramIdByOwner } from '../../context'
import {
  Wallet,
  executeTx,
  getVoteAccount,
  instanceOfWallet,
  transaction,
} from '@marinade.finance/web3js-common'
import {
  MARINADE_CONFIG_ADDRESS,
  initBondInstruction,
} from '@marinade.finance/validator-bonds-sdk'
import { Wallet as WalletInterface } from '@marinade.finance/web3js-common'
import { LAMPORTS_PER_SOL, PublicKey, Signer } from '@solana/web3.js'
import { INIT_BOND_LIMIT_UNITS } from '../../computeUnits'
import BN from 'bn.js'

export function installInitBond(program: Command) {
  program
    .command('init-bond')
    .description('Create a new bond account.')
    .option(
      '--config <pubkey>',
      'The config account that the bond is created under. ' +
        `(default: ${MARINADE_CONFIG_ADDRESS.toBase58()})`,
      parsePubkey
    )
    .requiredOption(
      '--vote-account <pubkey>',
      'Validator vote account that this bond is bound to',
      parsePubkey
    )
    .option(
      '--validator-identity <keypair_or_ledger_or_pubkey>',
      'Validator identity linked to the vote account. ' +
        'Permission-ed execution requires the validator identity signature, possible possible to configure --bond-authority. ' +
        'Permission-less execution requires no signature, bond account configuration is possible later with validator identity signature (default: NONE)',
      parseWalletOrPubkey
    )
    .option(
      '--bond-authority <pubkey>',
      'Authority that is permitted to operate with bond account. ' +
        'Only possible to set in permission-ed mode (see above, default: vote account validator identity)',
      parsePubkeyOrPubkeyFromWallet
    )
    .option(
      '--rent-payer <keypair_or_ledger_or_pubkey>',
      'Rent payer for the account creation (default: wallet keypair)',
      parseWalletOrPubkey
    )
    .option(
      '--cpmpe <number>',
      'Cost per mille per epoch, in lamports. The maximum amount of lamports the validator desires to pay for each 1000 delegated SOLs per epoch. (default: 0)',
      value => new BN(value, 10)
    )
    .option(
      '--max-stake-wanted <number>',
      'The maximum stake amount, in SOL, that the validator wants to be delegated to them (default: 0).',
      value => new BN(value, 10)
    )
    .action(
      async ({
        config,
        voteAccount,
        validatorIdentity,
        bondAuthority,
        rentPayer,
        cpmpe = new BN(0),
        maxStakeWanted = new BN(0),
      }: {
        config?: Promise<PublicKey>
        voteAccount: Promise<PublicKey>
        validatorIdentity?: Promise<WalletInterface | PublicKey>
        bondAuthority: Promise<PublicKey>
        rentPayer?: Promise<WalletInterface | PublicKey>
        cpmpe: BN
        maxStakeWanted: BN
      }) => {
        await manageInitBond({
          config: await config,
          voteAccount: await voteAccount,
          validatorIdentity: await validatorIdentity,
          bondAuthority: await bondAuthority,
          rentPayer: await rentPayer,
          cpmpe,
          maxStakeWanted,
        })
      }
    )
}

async function manageInitBond({
  config = MARINADE_CONFIG_ADDRESS,
  voteAccount,
  validatorIdentity,
  bondAuthority,
  rentPayer,
  cpmpe,
  maxStakeWanted,
}: {
  config?: PublicKey
  voteAccount: PublicKey
  validatorIdentity?: WalletInterface | PublicKey
  bondAuthority: PublicKey
  rentPayer?: WalletInterface | PublicKey
  cpmpe: BN
  maxStakeWanted: BN
}) {
  const {
    program,
    provider,
    logger,
    computeUnitPrice,
    simulate,
    printOnly,
    wallet,
    confirmationFinality,
    confirmWaitTime,
    skipPreflight,
  } = await setProgramIdByOwner(config)

  const tx = await transaction(provider)
  const signers: (Signer | Wallet)[] = [wallet]

  rentPayer = rentPayer ?? wallet.publicKey
  if (instanceOfWallet(rentPayer)) {
    signers.push(rentPayer)
    rentPayer = rentPayer.publicKey
  }
  if (instanceOfWallet(validatorIdentity)) {
    signers.push(validatorIdentity)
    validatorIdentity = validatorIdentity.publicKey
  }

  if (bondAuthority === undefined) {
    // when not defined the bondAuthority is the validator identity
    const voteAccountData = await getVoteAccount(provider, voteAccount)
    bondAuthority = voteAccountData.account.data.nodePubkey
  }

  const { instruction, bondAccount } = await initBondInstruction({
    program,
    configAccount: config,
    bondAuthority,
    voteAccount,
    validatorIdentity,
    rentPayer,
    cpmpe,
    maxStakeWanted: maxStakeWanted.mul(new BN(LAMPORTS_PER_SOL)),
  })
  tx.add(instruction)

  logger.info(
    `Initializing bond account ${bondAccount.toBase58()} (finalization may take seconds)`
  )
  await executeTx({
    connection: provider.connection,
    transaction: tx,
    errMessage:
      `'Failed to init bond account ${bondAccount.toBase58()}` +
      ` of config ${config.toBase58()}`,
    signers,
    logger,
    computeUnitLimit: INIT_BOND_LIMIT_UNITS,
    computeUnitPrice,
    simulate,
    printOnly,
    confirmOpts: confirmationFinality,
    confirmWaitTime,
    sendOpts: { skipPreflight },
  })
  logger.info(
    `Bond account ${bondAccount.toBase58()} of config ${config.toBase58()} successfully created`
  )
}
