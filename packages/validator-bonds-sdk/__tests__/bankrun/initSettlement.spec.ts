import {
  Bond,
  Errors,
  ValidatorBondsProgram,
  getBond,
  getSettlement,
  getSettlementClaimsBySettlement,
  initSettlementInstruction,
  settlementAddress,
  settlementClaimsAddress,
  settlementStakerAuthority,
} from '../../src'
import {
  BankrunExtendedProvider,
  assertNotExist,
  currentEpoch,
} from '@marinade.finance/bankrun-utils'
import {
  executeInitBondInstruction,
  executeInitConfigInstruction,
} from '../utils/testTransactions'
import { ProgramAccount } from '@coral-xyz/anchor'
import { Keypair, PublicKey } from '@solana/web3.js'
import { createVoteAccount } from '../utils/staking'
import { verifyError } from '@marinade.finance/anchor-common'
import { initBankrunTest } from './bankrun'
import { HEADER_DATA_SIZE } from '../../src/settlementClaims'

describe('Validator Bonds init settlement', () => {
  let provider: BankrunExtendedProvider
  let program: ValidatorBondsProgram
  let configAccount: PublicKey
  let bond: ProgramAccount<Bond>
  let operatorAuthority: Keypair
  let validatorIdentity: Keypair
  let voteAccount: PublicKey

  beforeAll(async () => {
    ;({ provider, program } = await initBankrunTest())
  })

  beforeEach(async () => {
    ;({ configAccount, operatorAuthority } = await executeInitConfigInstruction(
      {
        program,
        provider,
      }
    ))
    ;({ voteAccount, validatorIdentity } = await createVoteAccount({
      provider,
    }))
    const { bondAccount } = await executeInitBondInstruction({
      program,
      provider,
      configAccount,
      voteAccount,
      validatorIdentity,
    })
    bond = {
      publicKey: bondAccount,
      account: await getBond(program, bondAccount),
    }
  })

  it('init settlement', async () => {
    const merkleRoot = Buffer.from(
      Array.from({ length: 32 }, () => Math.floor(Math.random() * 256))
    )
    const epochNow = await currentEpoch(provider)
    const rentCollector = Keypair.generate().publicKey
    const { instruction, settlementAccount, epoch } =
      await initSettlementInstruction({
        program,
        bondAccount: bond.publicKey,
        operatorAuthority,
        merkleRoot,
        maxMerkleNodes: 1,
        maxTotalClaim: 3,
        voteAccount,
        epoch: epochNow,
        configAccount,
        rentCollector,
      })
    await provider.sendIx([operatorAuthority], instruction)

    expect(epoch.toString()).toEqual(epochNow.toString())

    const [settlementAddr, bump] = settlementAddress(
      bond.publicKey,
      merkleRoot,
      epoch,
      program.programId
    )
    expect(settlementAddr).toEqual(settlementAccount)
    const [authorityAddr, authorityBump] = settlementStakerAuthority(
      settlementAccount,
      program.programId
    )

    const settlementData = await getSettlement(program, settlementAccount)
    expect(settlementData.bond).toEqual(bond.publicKey)
    expect(settlementData.bumps.pda).toEqual(bump)
    expect(settlementData.bumps.stakerAuthority).toEqual(authorityBump)
    expect(settlementData.stakerAuthority).toEqual(authorityAddr)
    expect(settlementData.epochCreatedFor).toEqual(epoch)
    expect(settlementData.maxMerkleNodes).toEqual(1)
    expect(settlementData.maxTotalClaim).toEqual(3)
    expect(settlementData.merkleRoot).toEqual(Array.from(merkleRoot))
    expect(settlementData.merkleNodesClaimed).toEqual(0)
    expect(settlementData.lamportsFunded).toEqual(0)
    expect(settlementData.lamportsClaimed).toEqual(0)
    expect(settlementData.rentCollector).toEqual(rentCollector)
    expect(settlementData.splitRentAmount).toEqual(0)
    expect(settlementData.splitRentCollector).toEqual(null)

    const settlementAccountInfo =
      await provider.connection.getAccountInfo(settlementAccount)
    console.log(
      'settlement account length',
      settlementAccountInfo?.data.byteLength
    )
    // not account change size expected
    expect(settlementAccountInfo?.data.byteLength).toEqual(328)

    const [settlementClaimsAddr] = settlementClaimsAddress(
      settlementAccount,
      program.programId
    )
    const settlementClaimsAccountInfo =
      await provider.connection.getAccountInfo(settlementClaimsAddr)
    expect(settlementClaimsAccountInfo).not.toBeNull()
    // the size for bitmap is calculated to be at least 8 bytes
    expect(settlementClaimsAccountInfo?.data.byteLength).toEqual(
      HEADER_DATA_SIZE + 8
    )

    const settlementClaims = await getSettlementClaimsBySettlement(
      program,
      settlementAccount
    )
    expect(settlementClaims.bitmap.bitmapAsBits().length).toEqual(1)
  })

  it('cannot init settlement with wrong buffer size', async () => {
    const merkleRoot = Buffer.from(
      Array.from({ length: 30 }, () => Math.floor(Math.random() * 256))
    )
    const { instruction, settlementAccount } = await initSettlementInstruction({
      program,
      bondAccount: bond.publicKey,
      merkleRoot,
      maxMerkleNodes: 1,
      maxTotalClaim: 3,
      voteAccount,
      configAccount,
      epoch: await currentEpoch(provider),
    })
    try {
      await provider.sendIx([operatorAuthority], instruction)
      throw new Error('failure; expected wrong seeds constraint')
    } catch (e) {
      // Error Code: ConstraintSeeds. Error Number: 2006. Error Message: A seeds constraint was violated.
      if (!(e as Error).message.includes('custom program error: 0x7d6')) {
        throw e
      }
    }
    assertNotExist(provider, settlementAccount)
  })

  it('init settlement with future epoch', async () => {
    const merkleRoot = Buffer.alloc(32)
    const futureEpoch = (await currentEpoch(provider)) + 2024
    const { instruction, settlementAccount } = await initSettlementInstruction({
      program,
      bondAccount: bond.publicKey,
      operatorAuthority,
      merkleRoot,
      maxMerkleNodes: 1,
      maxTotalClaim: 3,
      voteAccount,
      configAccount,
      epoch: futureEpoch,
    })
    await provider.sendIx([operatorAuthority], instruction)
    expect(
      await provider.connection.getAccountInfo(settlementAccount)
    ).not.toBeNull()
  })

  it('cannot init settlement with wrong authority', async () => {
    const merkleRoot = Buffer.alloc(32)
    const wrongOperator = Keypair.generate()
    const { instruction, settlementAccount } = await initSettlementInstruction({
      program,
      bondAccount: bond.publicKey,
      operatorAuthority: wrongOperator,
      merkleRoot,
      maxMerkleNodes: 1,
      maxTotalClaim: 3,
      voteAccount,
      epoch: await currentEpoch(provider),
      configAccount,
    })
    try {
      await provider.sendIx([wrongOperator], instruction)
      throw new Error('failure; expected wrong operator authority')
    } catch (e) {
      verifyError(e, Errors, 6003, 'operator authority signature')
    }
    assertNotExist(provider, settlementAccount)
  })
})
