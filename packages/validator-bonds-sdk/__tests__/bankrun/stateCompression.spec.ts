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
  warpToNextSlot,
} from '@marinade.finance/bankrun-utils'
import {
  executeInitBondInstruction,
  executeInitConfigInstruction,
  executeInitSettlement,
} from '../utils/testTransactions'
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  TransactionInstruction,
} from '@solana/web3.js'
import {
  createBondsFundedStakeAccount,
  createDelegatedStakeAccount,
  createVoteAccount,
} from '../utils/staking'
import {
  signer,
  createUserAndFund,
  publicKey,
} from '@marinade.finance/web3js-common'
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
  ConcurrentMerkleTreeAccount,
  createAllocTreeIx,
  createAppendIx,
  createInitEmptyMerkleTreeIx,
  createReplaceIx,
  createVerifyLeafIx,
  emptyNode,
  MerkleTree,
  ValidDepthSizePair,
} from '@solana/spl-account-compression'
import { concurrentMerkleTreeBeetFactory } from '@solana/spl-account-compression/src/types'
import assert from 'assert'

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

  // see doc: https://solana.com/docs/advanced/state-compression#sizing-a-concurrent-merkle-tree
  it.only('state compression', async () => {
    // const cmtKeypair = Keypair.generate()
    // const depthSizePair: ValidDepthSizePair = { maxDepth: 3, maxBufferSize: 8 }

    // // const concurrentTree = concurrentMerkleTreeBeetFactory(3, 8);

    // const pubkeys = Array.from({ length: 200 }, () => Keypair.generate().publicKey)
    // const possibleDepth = Math.ceil(Math.log2(pubkeys.length));
    // let pair = ALL_DEPTH_SIZE_PAIRS.filter(pair => pair.maxDepth >= possibleDepth)
    //   .sort((a, b) => a.maxDepth - b.maxDepth)
    // if (pair.length == 0) {
    //   throw new Error('No valid depth for concurrent merkle tree')
    // }
    // const maxDepth = pair[0].maxDepth;
    // const maxBufferSize = pair[0].maxBufferSize;
    // let merkleTree = MerkleTree.sparseMerkleTreeFromLeaves(pubkeys.map(pk => pk.toBuffer()), maxDepth);
    // if (pair.length != 1) {
    //   throw new Error('Invalid depth for concurrent merkle tree')
    // }
    // const merkleTreeAccount = concurrentMerkleTreeBeetFactory(maxDepth, maxBufferSize);
    // merkleTreeAccount.serialize(merkleTree);
    // ConcurrentMerkleTreeAccount.deserialize(merkleTreeAccount.serialize(merkleTree));

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

    // const depthSizePair: ValidDepthSizePair = { maxDepth: 3, maxBufferSize: 8 }
    // // const concurrentTree = concurrentMerkleTreeBeetFactory(3, 8);
    const pubkeys = Array.from(
      { length: 200 },
      () => Keypair.generate().publicKey
    )
    // just some, but static public key to easier check
    pubkeys[0] = new PublicKey([
      217, 216, 212, 72, 68, 213, 78, 169, 193, 177, 56, 43, 160, 250, 225, 136,
      36, 95, 13, 209, 119, 52, 31, 175, 159, 69, 89, 208, 36, 169, 123, 21,
    ])

    const possibleDepth = Math.ceil(Math.log2(pubkeys.length))
    const pair = ALL_DEPTH_SIZE_PAIRS.filter(
      pair => pair.maxDepth >= possibleDepth
    ).sort((a, b) => a.maxDepth - b.maxDepth)
    if (pair.length == 0) {
      throw new Error('No valid depth for concurrent merkle tree')
    }

    // see  https://github.com/solana-labs/solana-program-library/blob/8f50c6fabc6ec87ada229e923030381f573e0aed/account-compression/sdk/tests/utils.ts#L52
    const cmtKeypair = Keypair.generate()
    // const offChainTree = new MerkleTree(pubkeys.map(pk => pk.toBuffer()));
    const canopyDepth = 30 // let's say 0 and we will see :-)

    // the first the lowest depthSizePair that can fit the tree
    const depthSizePair = pair[0]

    const authorityGuy = Keypair.generate()

    const allocAccountIx = await createAllocTreeIx(
      provider.connection,
      cmtKeypair.publicKey,
      provider.walletPubkey,
      depthSizePair,
      canopyDepth
    )
    const ixs = [
      allocAccountIx,
      createInitEmptyMerkleTreeIx(
        cmtKeypair.publicKey,
        authorityGuy.publicKey,
        depthSizePair
      ),
    ]

    await provider.sendIx([cmtKeypair, authorityGuy], ...ixs)

    console.log(`Consulting ${pubkeys.length} pubkeys`)
    let appendedPubkeys: PublicKey[] = []
    const batchSize = 5
    // for (let i = 0; i < pubkeys.length; i += batchSize) {
    for (let i = pubkeys.length - 1; i > -5; i -= batchSize) {
      const pubkeyBatch = pubkeys.slice(i, i + batchSize > 0 ? i + batchSize : 0)
      if (pubkeyBatch.length === 0) {
        break
      }
      const appendIxs: TransactionInstruction[] = pubkeyBatch
        .map(pk => pk.toBuffer())
        .map(leaf => {
          return createAppendIx(
            cmtKeypair.publicKey,
            authorityGuy.publicKey,
            leaf
          )
        })
      appendedPubkeys = appendedPubkeys.concat(pubkeyBatch)
      await provider.sendIx([authorityGuy], ...appendIxs)

      const offChainTree = MerkleTree.sparseMerkleTreeFromLeaves(
        appendedPubkeys.map(pk => pk.toBuffer()),
        depthSizePair.maxDepth
      )
      const cmt = await ConcurrentMerkleTreeAccount.fromAccountAddress(
        provider.connection,
        cmtKeypair.publicKey,
        'confirmed'
      )
      const accountInfo = await provider.connection.getAccountInfo(
        cmtKeypair.publicKey
      )
      // printing concurrent merkle tree:
      // console.log(`>>> ${i}:`, JSON.stringify(cmt))
      // printing account info:
      // console.log(
      //   `>>> accountInfo RUN: ${i}, size: ${accountInfo?.data.length}, lamports: ${accountInfo?.lamports}:`,
      //   accountInfo?.data.map(x => x).join(",")
      // )

      console.log('-------------------------')
      assertCMTProperties(
        cmt,
        depthSizePair.maxDepth,
        depthSizePair.maxBufferSize,
        authorityGuy.publicKey,
        offChainTree.getRoot()
      )
    }

    const offChainTree = MerkleTree.sparseMerkleTreeFromLeaves(
      pubkeys.map(pk => pk.toBuffer()),
      depthSizePair.maxDepth
    )
    const firstPubkey = pubkeys[0]
    const possibleIndexes = offChainTree.leaves
      .filter(leaf => {
        // console.log('|', leaf.node.join(","), pubkeys[0].toBuffer().join(","), leaf.node.equals(pubkeys[0].toBuffer()))
        return leaf.node.equals(firstPubkey.toBuffer())
      })
      .map(l => l.id)
    assert(
      possibleIndexes.length === 1,
      'First pubkey not found in offChainTree'
    )
    const firstIndex = possibleIndexes[0]
    console.log(
      'first key, index:',
      firstIndex,
      'first pubkey',
      firstPubkey.toBase58(),
      firstPubkey.toBuffer().join(',')
    )

    const verifyIx = createVerifyLeafIx(
      cmtKeypair.publicKey,
      offChainTree.getProof(firstIndex)
      // to fail
      // {
      //   leafIndex: 2,
      //   leaf: firstPubkey.toBuffer(),
      //   root: offChainTree.getRoot(),
      //   proof: offChainTree.getProof(firstIndex).proof,
      // }
    )
    console.log('Verification of index:', firstIndex)
    await provider.sendIx([], verifyIx)
    const verify2Ix = createVerifyLeafIx(
      cmtKeypair.publicKey,
      offChainTree.getProof(pubkeys.length - 1)
    )
    console.log('Verification of index:', pubkeys.length - 1)
    await provider.sendIx([], verify2Ix)

    const replaceIx = createReplaceIx(
      cmtKeypair.publicKey,
      authorityGuy.publicKey,
      Buffer.alloc(32),
      offChainTree.getProof(firstIndex)
    )
    await provider.sendIx([authorityGuy], replaceIx)

    await warpToNextSlot(provider)
    try {
      await provider.sendIx([], verifyIx)
      throw new Error('Should have failed as the leaf was replaced')
    } catch (e) {
      // Error: Failed to process transaction: transport transaction error: Error processing Instruction 0: custom program error: 0x1771
      expect((e as Error).message).toContain('0x1771')
    }
    // still possible to verify the leaf despite the root has changed after replace ix
    await provider.sendIx([], verify2Ix)
  })

  function assertCMTProperties(
    onChainCMT: ConcurrentMerkleTreeAccount,
    expectedMaxDepth: number,
    expectedMaxBufferSize: number,
    expectedAuthority: PublicKey,
    expectedRoot: Buffer,
    expectedCanopyDepth?: number
  ) {
    assert(
      onChainCMT.getMaxDepth() === expectedMaxDepth,
      `Max depth does not match ${onChainCMT.getMaxDepth()}, expected ${expectedMaxDepth}`
    )
    assert(
      onChainCMT.getMaxBufferSize() === expectedMaxBufferSize,
      `Max buffer size does not match ${onChainCMT.getMaxBufferSize()}, expected ${expectedMaxBufferSize}`
    )
    assert(
      onChainCMT.getAuthority().equals(expectedAuthority),
      'Failed to write auth pubkey'
    )
    assert(
      onChainCMT.getCurrentRoot().equals(expectedRoot),
      'On chain root does not match root passed in instruction'
    )
    if (expectedCanopyDepth) {
      assert(
        onChainCMT.getCanopyDepth() === expectedCanopyDepth,
        'On chain canopy depth does not match expected canopy depth'
      )
    }
  }

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
