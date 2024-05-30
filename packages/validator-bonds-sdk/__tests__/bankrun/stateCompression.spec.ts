import {
  ValidatorBondsProgram,
  claimSettlementInstruction,
  fundSettlementInstruction,
  getSettlement,
  getSettlementClaim,
  settlementClaimAddress,
} from '../../src'
import {
  BankrunExtendedProvider,
  currentEpoch,
  warpToNextEpoch,
} from '@marinade.finance/bankrun-utils'
import {
  executeInitBondInstruction,
  executeInitConfigInstruction,
  executeInitSettlement,
} from '../utils/testTransactions'
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js'
import {
  createBondsFundedStakeAccount,
  createDelegatedStakeAccount,
  createVoteAccount,
} from '../utils/staking'
import { signer, createUserAndFund } from '@marinade.finance/web3js-common'
import {
  MERKLE_ROOT_VOTE_ACCOUNT_1_BUF,
  configAccountKeypair,
  createWithdrawerUsers,
  totalClaimVoteAccount1,
  treeNodeBy,
  voteAccount1Keypair,
  withdrawer1,
} from '../utils/merkleTreeTestData'
import { getFirstSlotOfEpoch, initBankrunTest } from './bankrun'
import {
  ALL_DEPTH_SIZE_PAIRS,
  createAllocTreeIx,
  createAppendIx,
  createInitEmptyMerkleTreeIx,
  MerkleTree,
  ValidDepthSizePair,
} from '@solana/spl-account-compression'
import { concurrentMerkleTreeBeetFactory } from '@solana/spl-account-compression/src/types'

describe('Validator Bonds claim settlement testing compression', () => {
  const epochsToClaimSettlement = 4
  // the test activates the stake account, we need to set slots to be
  // after the start of the next epoch when the stake account is active as we warped there
  let slotsToStartSettlementClaiming: number
  let settlement1ClaimingExpires: bigint
  let provider: BankrunExtendedProvider
  let program: ValidatorBondsProgram
  let configAccount: PublicKey
  let operatorAuthority: Keypair
  let validatorIdentity1: Keypair
  let voteAccount1: PublicKey
  let settlementAccount1: PublicKey
  let settlementEpoch: number
  let rentCollector: Keypair
  let stakeAccount1: PublicKey

  beforeAll(async () => {
    ;({ provider, program } = await initBankrunTest())
    const epochNow = await currentEpoch(provider)
    const firstSlotOfEpoch = await getFirstSlotOfEpoch(provider, epochNow)
    const firstSlotOfNextEpoch = await getFirstSlotOfEpoch(
      provider,
      epochNow + 1
    )
    slotsToStartSettlementClaiming =
      Number(firstSlotOfNextEpoch - firstSlotOfEpoch) + 3
    ;({ configAccount, operatorAuthority } = await executeInitConfigInstruction(
      {
        program,
        provider,
        epochsToClaimSettlement,
        slotsToStartSettlementClaiming,
        configAccountKeypair: configAccountKeypair,
      }
    ))
    ;({ voteAccount: voteAccount1, validatorIdentity: validatorIdentity1 } =
      await createVoteAccount({
        voteAccount: voteAccount1Keypair,
        provider,
      }))
    await executeInitBondInstruction({
      program,
      provider,
      configAccount,
      voteAccount: voteAccount1,
      validatorIdentity: validatorIdentity1,
    })

    rentCollector = Keypair.generate()
    settlementEpoch = await currentEpoch(provider)
    ;({ settlementAccount: settlementAccount1 } = await executeInitSettlement({
      configAccount,
      program,
      provider,
      voteAccount: voteAccount1,
      operatorAuthority,
      currentEpoch: settlementEpoch,
      rentCollector: rentCollector.publicKey,
      merkleRoot: MERKLE_ROOT_VOTE_ACCOUNT_1_BUF,
      maxMerkleNodes: 1,
      maxTotalClaim: totalClaimVoteAccount1,
    }))
    const settlement1Slot = (await getSettlement(program, settlementAccount1))
      .slotCreatedAt
    settlement1ClaimingExpires =
      BigInt(settlement1Slot.toString()) +
      BigInt(slotsToStartSettlementClaiming)
    stakeAccount1 = await createBondsFundedStakeAccount({
      program,
      provider,
      configAccount,
      voteAccount: voteAccount1,
      lamports: totalClaimVoteAccount1.toNumber() + LAMPORTS_PER_SOL * 5,
    })

    await warpToNextEpoch(provider) // activate stake account

    const { instruction: fundIx1, splitStakeAccount: split1 } =
      await fundSettlementInstruction({
        program,
        settlementAccount: settlementAccount1,
        stakeAccount: stakeAccount1,
      })
    await provider.sendIx([signer(split1), operatorAuthority], fundIx1)
    await createWithdrawerUsers(provider)
  })

  it.only('state compression', async () => {
    const cmtKeypair = Keypair.generate()
    const depthSizePair: ValidDepthSizePair = { maxDepth: 3, maxBufferSize: 8 }

    // const concurrentTree = concurrentMerkleTreeBeetFactory(3, 8);

    const pubkeys = Array.from({ length: 200 }, () => Keypair.generate().publicKey)
    const possibleDepth = Math.ceil(Math.log2(pubkeys.length));
    let pair = ALL_DEPTH_SIZE_PAIRS.filter(pair => pair.maxDepth >= possibleDepth)
      .sort((a, b) => a.maxDepth - b.maxDepth)
    if (pair.length == 0) {
      throw new Error('No valid depth for concurrent merkle tree')
    }
    const maxDepth = pair[0].maxDepth;
    const maxBufferSize = pair[0].maxBufferSize;
    let merkleTree = MerkleTree.sparseMerkleTreeFromLeaves(pubkeys.map(pk => pk.toBuffer()), maxDepth);
    if (pair.length != 1) {
      throw new Error('Invalid depth for concurrent merkle tree')
    }
    const merkleTreeAccount = concurrentMerkleTreeBeetFactory(maxDepth, maxBufferSize);
    // merkleTreeAccount.serialize(merkleTree);
    ConcurrentMerkleTreeAccount.deserialize(merkleTreeAccount.serialize(merkleTree));


    // const allocAccountIx = await createAllocTreeIx(
    //   provider.connection,
    //   cmtKeypair.publicKey,
    //   provider.walletPubkey,
    //   depthSizePair,
    //   // canopyDepth
    //   0
    // )
    // const ixs = [
    //   allocAccountIx,
    //   createInitEmptyMerkleTreeIx(
    //     cmtKeypair.publicKey,
    //     provider.walletPubkey,
    //     depthSizePair
    //   ),
    // ]
    // await provider.sendIx([cmtKeypair], ...ixs)
  })

  it('claim settlement with state compression', async () => {
    const treeNode1Withdrawer1 = treeNodeBy(voteAccount1, withdrawer1)
    const stakeAccountLamportsBefore = 123 * LAMPORTS_PER_SOL
    const stakeAccountTreeNode1Withdrawer1 = await createDelegatedStakeAccount({
      provider,
      lamports: stakeAccountLamportsBefore,
      voteAccount: voteAccount1,
      staker: treeNode1Withdrawer1.treeNode.stakeAuthority,
      withdrawer: treeNode1Withdrawer1.treeNode.withdrawAuthority,
    })
    provider.context.warpToSlot(settlement1ClaimingExpires)
    const rentPayer = await createUserAndFund({
      provider,
      lamports: LAMPORTS_PER_SOL,
    })
    await warpToNextEpoch(provider) // deactivate stake account

    const { instruction, settlementClaimAccount, merkleTree } =
      await claimSettlementInstruction({
        program,
        claimAmount: treeNode1Withdrawer1.treeNode.data.claim,
        merkleProof: treeNode1Withdrawer1.proof,
        settlementAccount: settlementAccount1,
        stakeAccountFrom: stakeAccount1,
        stakeAccountTo: stakeAccountTreeNode1Withdrawer1,
        stakeAccountStaker: treeNode1Withdrawer1.treeNode.stakeAuthority,
        stakeAccountWithdrawer: treeNode1Withdrawer1.treeNode.withdrawAuthority,
        rentPayer: rentPayer,
      })

    await provider.sendIx([signer(rentPayer), merkleTree], instruction)

    const stakeAccountInfo = await provider.connection.getAccountInfo(
      stakeAccountTreeNode1Withdrawer1
    )
    expect(stakeAccountInfo?.lamports).toEqual(
      stakeAccountLamportsBefore +
        treeNode1Withdrawer1.treeNode.data.claim.toNumber()
    )

    const [settlementClaimAddr, bump] = settlementClaimAddress(
      {
        settlement: settlementAccount1,
        stakeAccountStaker: treeNode1Withdrawer1.treeNode.stakeAuthority,
        stakeAccountWithdrawer: treeNode1Withdrawer1.treeNode.withdrawAuthority,
        claim: treeNode1Withdrawer1.treeNode.data.claim,
      },
      program.programId
    )
    expect(settlementClaimAccount).toEqual(settlementClaimAddr)
    const settlementClaim = await getSettlementClaim(
      program,
      settlementClaimAccount
    )
    expect(settlementClaim.amount).toEqual(
      treeNode1Withdrawer1.treeNode.data.claim
    )
  })
})
