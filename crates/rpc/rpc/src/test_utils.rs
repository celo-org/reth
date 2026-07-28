use alloy_consensus::Header;
use alloy_primitives::{address, Address, U256};
use reth_ethereum_primitives::Block;
use reth_evm::{ConfigureEvm, Database, EvmEnvFor, EvmFor, ExecutionCtxFor, InspectorFor};
use reth_evm_ethereum::EthEvmConfig;
use reth_primitives_traits::SealedBlock;
use std::{
    any::Any,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

pub(crate) const CONTEXT_MARKER: Address = address!("00000000000000000000000000000000000000bb");

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordedReplayContext {
    pub(crate) id: usize,
    pub(crate) marker_balance: Option<U256>,
    pub(crate) block_number: U256,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReplayContextRecorder {
    next_id: Arc<AtomicUsize>,
    captures: Arc<Mutex<Vec<RecordedReplayContext>>>,
    seeds: Arc<Mutex<Vec<RecordedReplayContext>>>,
}

impl ReplayContextRecorder {
    pub(crate) fn evm_config(&self, inner: EthEvmConfig) -> RecordingEvmConfig {
        RecordingEvmConfig { inner, recorder: self.clone() }
    }

    pub(crate) fn captures(&self) -> Vec<RecordedReplayContext> {
        self.captures.lock().unwrap().clone()
    }

    pub(crate) fn seeds(&self) -> Vec<RecordedReplayContext> {
        self.seeds.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RecordingEvmConfig {
    inner: EthEvmConfig,
    recorder: ReplayContextRecorder,
}

impl ConfigureEvm for RecordingEvmConfig {
    type Primitives = <EthEvmConfig as ConfigureEvm>::Primitives;
    type Error = <EthEvmConfig as ConfigureEvm>::Error;
    type NextBlockEnvCtx = <EthEvmConfig as ConfigureEvm>::NextBlockEnvCtx;
    type BlockExecutorFactory = <EthEvmConfig as ConfigureEvm>::BlockExecutorFactory;
    type BlockAssembler = <EthEvmConfig as ConfigureEvm>::BlockAssembler;

    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        self.inner.block_executor_factory()
    }

    fn block_assembler(&self) -> &Self::BlockAssembler {
        self.inner.block_assembler()
    }

    fn evm_env(&self, header: &Header) -> Result<EvmEnvFor<Self>, Self::Error> {
        self.inner.evm_env(header)
    }

    fn next_evm_env(
        &self,
        parent: &Header,
        attributes: &Self::NextBlockEnvCtx,
    ) -> Result<EvmEnvFor<Self>, Self::Error> {
        self.inner.next_evm_env(parent, attributes)
    }

    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<Block>,
    ) -> Result<ExecutionCtxFor<'a, Self>, Self::Error> {
        self.inner.context_for_block(block)
    }

    fn context_for_next_block(
        &self,
        parent: &reth_primitives_traits::SealedHeader,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<ExecutionCtxFor<'_, Self>, Self::Error> {
        self.inner.context_for_next_block(parent, attributes)
    }

    fn capture_block_replay_ctx<DB: Database>(
        &self,
        db: &mut DB,
        evm_env: &EvmEnvFor<Self>,
    ) -> Option<Box<dyn Any + Send>> {
        let ctx = RecordedReplayContext {
            id: self.recorder.next_id.fetch_add(1, Ordering::SeqCst),
            marker_balance: db.basic(CONTEXT_MARKER).ok().flatten().map(|account| account.balance),
            block_number: evm_env.block_env.number,
        };
        self.recorder.captures.lock().unwrap().push(ctx.clone());
        Some(Box::new(ctx))
    }

    fn seed_block_replay_ctx<DB, I>(&self, _evm: &mut EvmFor<Self, DB, I>, ctx: &(dyn Any + Send))
    where
        DB: Database,
        I: InspectorFor<Self, DB>,
    {
        let ctx = ctx.downcast_ref::<RecordedReplayContext>().expect("recorded context type");
        self.recorder.seeds.lock().unwrap().push(ctx.clone());
    }
}
