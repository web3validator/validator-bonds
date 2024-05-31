import {
  ValidatorBondsProgram,
  checkAndGetBondAddress,
  getProgram,
} from '../../src'
import { Keypair, PublicKey } from '@solana/web3.js'
import {
  BankrunExtendedProvider,
  testInit,
  warpToNextEpoch,
} from '@marinade.finance/bankrun-utils'
import { delegatedStakeAccount } from '../utils/staking'
import {
  executeFundBondInstruction,
  executeInitBondInstruction,
} from '../utils/testTransactions'
import 'reflect-metadata'
import { PROGRAM_ID as SPL_ACCOUNT_COMPRESSION_PROGRAM_ID } from '@solana/spl-account-compression'

export async function initBankrunTest(programId?: PublicKey): Promise<{
  program: ValidatorBondsProgram
  provider: BankrunExtendedProvider
}> {
  const programs = [
    {
      // https://github.com/solana-labs/solana-program-library/blob/master/account-compression/programs/account-compression/src/lib.rs
      // cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK
      pubkey: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
      // https://github.com/solana-labs/solana/blob/v1.18.14/program-test/src/lib.rs#L428
      name: 'spl_account_compression',
      path: './fixtures/programs/spl_account_compression.so',
    },
    {
      pubkey: new PublicKey('noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV'),
      name: 'spl_noop',
      path: './fixtures/programs/spl_noop.so',
    },
  ]
  const provider = await testInit({
    accountDirs: ['./fixtures/accounts/'],
    programs
  })
  return {
    program: getProgram({ connection: provider, programId }),
    provider,
  }
}

// this cannot be in generic testTransactions.ts because of warping requires BankrunProvider
export async function delegateAndFund({
  program,
  provider,
  lamports,
  voteAccount,
  bondAccount,
  configAccount,
}: {
  program: ValidatorBondsProgram
  provider: BankrunExtendedProvider
  lamports: number
  voteAccount?: PublicKey
  bondAccount?: PublicKey
  configAccount?: PublicKey
}): Promise<{
  stakeAccount: PublicKey
  bondAccount: PublicKey
  voteAccount: PublicKey
  validatorIdentity: Keypair | undefined
}> {
  const {
    stakeAccount,
    withdrawer,
    voteAccount: voteAccountDelegated,
    validatorIdentity,
  } = await delegatedStakeAccount({
    provider,
    lamports,
    voteAccountToDelegate: voteAccount,
  })
  if (bondAccount && configAccount) {
    const bondToCheck = checkAndGetBondAddress(
      undefined,
      configAccount,
      voteAccountDelegated,
      program.programId
    )
    expect(bondAccount).toEqual(bondToCheck)
  }
  if (
    bondAccount === undefined ||
    (await provider.connection.getAccountInfo(bondAccount)) === null
  ) {
    if (configAccount === undefined) {
      throw new Error('delegateAndFund: configAccount is required')
    }
    ;({ bondAccount } = await executeInitBondInstruction({
      program,
      provider,
      voteAccount: voteAccountDelegated,
      validatorIdentity,
      configAccount,
    }))
  }

  await warpToNextEpoch(provider) // activating stake account
  await executeFundBondInstruction({
    program,
    provider,
    bondAccount: bondAccount,
    stakeAccount,
    stakeAccountAuthority: withdrawer,
  })
  return {
    stakeAccount,
    bondAccount,
    voteAccount: voteAccountDelegated,
    validatorIdentity,
  }
}

// https://github.com/solana-labs/solana/blob/v1.17.7/sdk/program/src/epoch_schedule.rs#L29C1-L29C45
// https://github.com/solana-labs/solana/blob/v1.17.7/sdk/program/src/epoch_schedule.rs#L167
export async function getFirstSlotOfEpoch(
  provider: BankrunExtendedProvider,
  epoch: number
): Promise<bigint> {
  const epochBigInt = BigInt(epoch)
  const { slotsPerEpoch, firstNormalEpoch, firstNormalSlot } =
    provider.context.genesisConfig.epochSchedule
  let firstEpochSlot: bigint
  const MINIMUM_SLOTS_PER_EPOCH = 32
  if (epochBigInt <= firstNormalEpoch) {
    firstEpochSlot = BigInt((2 ** epoch - 1) * MINIMUM_SLOTS_PER_EPOCH)
  } else {
    firstEpochSlot =
      (epochBigInt - firstNormalEpoch) * slotsPerEpoch + firstNormalSlot
  }
  return firstEpochSlot
}
