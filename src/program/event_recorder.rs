use crate::{phoenix_log_authority, state::markets::MarketEvent};
use borsh::BorshSerialize;
use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};

use super::{
    assert_with_msg, checkers::phoenix_checkers::MarketAccountInfo, AuditLogHeader, PhoenixError,
    PhoenixInstruction, PhoenixLogContext, PhoenixMarketContext, PhoenixMarketEvent,
};

/// The maximum amount of data that can be sent through a CPI is 1280 bytes
const MAX_INNER_INSTRUCTION_SIZE: usize = 1280;

/// The number of bytes in a single AccountMeta struct
const LOG_IX_ACCOUNT_META_SIZE: usize = 34;

/// The header is used to decode the events from the client side
/// It contains the following metadata:
///
/// size (bytes)    description                  data type
/// -----------------------------------------------------
/// 1               log instruction enum         u8
/// 1               market event enum            u8
/// 1               current instruction enum     u8
/// 8               sequence number              u64
/// 8               timestamp                    i64
/// 8               slot                         u64
/// 32              market pubkey                Pubkey
/// 32              signer pubkey                Pubkey
/// 2               number of events in batch    u16
const HEADER_LEN: usize = 93;

/// The largest event is a fill event
/// It contains the following metadata:
///
/// size (bytes)    description                  data type
/// -----------------------------------------------------
/// 1               market event enum            u8
/// 2               index                        u16,
/// 32              maker_id                     Pubkey,
/// 8               order_sequence_number        u64,
/// 8               price_in_ticks               u64,
/// 8               base_lots_filled             u64,
/// 8               base_lots_remaining          u64,
const MAX_EVENT_SIZE: usize = 67;

/// This struct manages in internal state of market events. It is used to
/// track the current state of the event buffer and to serialize the
/// events into a buffer that can be sent to the log authority.
///
/// If it is at capacity, the `flush` method will need to be called to
/// CPI to the log instruction to log the events and drain the `event_buffer`.
///
/// This enables the program to only have to allocate heap memory once per instruction
pub(crate) struct EventRecorder<'info> {
    phoenix_program: AccountInfo<'info>,
    log_authority: AccountInfo<'info>,
    phoenix_instruction: PhoenixInstruction,

    /// This buffer is used to serialize the events without allocating new heap memory
    scratch_buffer: Vec<u8>,
    /// This instruction template is reused for each log CPI
    pub log_instruction: Instruction,
    /// This struct is used to track the state of the event buffer
    /// (number of events, pending events, current batch index etc.)
    state_tracker: EventStateTracker,
    error_code: Option<PhoenixError>,
}

impl<'info> EventRecorder<'info> {
    pub(crate) fn new<'a>(
        phoenix_log_context: PhoenixLogContext<'a, 'info>,
        phoenix_market_context: &PhoenixMarketContext<'a, 'info>,
        phoenix_instruction: PhoenixInstruction,
    ) -> Result<Self, ProgramError> {
        let PhoenixLogContext {
            phoenix_program,
            log_authority,
        } = phoenix_log_context;
        let PhoenixMarketContext {
            market_info,
            signer,
        } = phoenix_market_context;
        let header = market_info.get_header()?;
        let clock = Clock::get()?;

        // Serialize data to a static buffer to avoid vector resizing
        let mut data = Vec::with_capacity(MAX_INNER_INSTRUCTION_SIZE);
        data.push(PhoenixInstruction::Log as u8);
        PhoenixMarketEvent::Header(AuditLogHeader {
            instruction: phoenix_instruction as u8,
            sequence_number: header.market_sequence_number,
            timestamp: clock.unix_timestamp,
            slot: clock.slot,
            market: *market_info.key,
            signer: *signer.key,
            total_events: 0, // This will get overridden on each CPI
        })
        .serialize(&mut data)?;

        Ok(Self {
            phoenix_program: phoenix_program.as_ref().clone(),
            log_authority: log_authority.as_ref().clone(),
            phoenix_instruction,
            // Allocate 128 bytes for the event scratch buffer to prevent resizing
            scratch_buffer: Vec::with_capacity(MAX_EVENT_SIZE),
            log_instruction: Instruction {
                program_id: crate::id(),
                accounts: vec![AccountMeta::new_readonly(phoenix_log_authority::id(), true)],
                data,
            },
            state_tracker: EventStateTracker::default(),
            error_code: None,
        })
    }

    /// Records Phoenix events via CPI
    pub(crate) fn flush(&mut self) -> ProgramResult {
        let batch_size = self.state_tracker.get_batch_size();
        self.state_tracker.print_status();
        // Store the number of emitted events in the header to more easily decode the events
        // from the client side. "number of events in batch"
        self.log_instruction.data[(HEADER_LEN - 2)..HEADER_LEN]
            .copy_from_slice(&batch_size.to_le_bytes());
        invoke_signed(
            &self.log_instruction,
            &[
                self.phoenix_program.as_ref().clone(),
                self.log_authority.as_ref().clone(),
            ],
            &[&[b"log", &[phoenix_log_authority::bump()]]],
        )?;
        self.log_instruction.data.drain(HEADER_LEN..);
        self.state_tracker.process_events();
        Ok(())
    }

    /// Adds a MarketEvent to the current instruction. If the instruction data
    /// length exceeds the maximum inner instruction size, the events are recorded
    /// via CPI
    pub(crate) fn add_event(&mut self, event: MarketEvent<Pubkey>) {
        if self.error_code.is_some() {
            return;
        }
        // By serialzing into an existing buffer, we avoid allocating a new vector
        let mut event = PhoenixMarketEvent::from(event);
        event.set_index(self.state_tracker.events_added);

        // This should always be false, but we check just in case
        if !self.scratch_buffer.is_empty() {
            self.error_code = Some(PhoenixError::NonEmptyScratchBuffer);
            return;
        }

        // This should always be false, but we check just in case
        if event.serialize(&mut self.scratch_buffer).is_err() {
            self.error_code = Some(PhoenixError::FailedToSerializeEvent);
            return;
        }

        // Flush the buffer if the data length exceeds the maximum inner instruction size
        let data_len = self.log_instruction.data.len() + self.scratch_buffer.len();
        if data_len + LOG_IX_ACCOUNT_META_SIZE > MAX_INNER_INSTRUCTION_SIZE && self.flush().is_err()
        {
            // This should never happen because the program should terminate in `self.flush` before
            // fully evaluating the condition above
            self.error_code = Some(PhoenixError::FailedToFlushBuffer);
            return;
        }
        self.log_instruction
            .data
            .extend_from_slice(&self.scratch_buffer);
        self.state_tracker.add_event();
        // We drain the buffer to avoid having to reallocate memory
        self.scratch_buffer.drain(..);
    }

    /// Increments the market sequence number and then emits the events
    pub(crate) fn increment_market_sequence_number_and_flush(
        &mut self,
        market_info: MarketAccountInfo<'_, 'info>,
    ) -> ProgramResult {
        if let Some(err) = self.error_code {
            phoenix_log!("ERROR: Event recorder failed to record events: {}", err);
            return Err(err.into());
        }
        if market_info.data_is_empty() {
            assert_with_msg(
                self.phoenix_instruction == PhoenixInstruction::ChangeMarketStatus,
                ProgramError::InvalidInstructionData,
                "The only instruction that can be used to delete a market is ChangeMarketStatus",
            )?;
        } else {
            market_info.get_header_mut()?.increment_sequence_number();
        };
        if self.state_tracker.has_events_to_process() {
            self.flush()?;
        }
        Ok(())
    }
}

/// This is a helper struct that tracks the state of events and event batching for the current instruction
#[derive(Default)]
pub(crate) struct EventStateTracker {
    /// The total number of event batches for the current instruction
    batch_index: usize,

    /// The number of emitted events in the current instruction
    events_emitted: u16,

    /// The number of events added in the current instruction
    events_added: u16,
}

impl EventStateTracker {
    pub(crate) fn print_status(&self) {
        phoenix_log!(
            "Sending batch {} with header and {} market events, total events sent: {}",
            self.batch_index + 1,
            self.get_batch_size(),
            self.events_added,
        );
    }

    pub(crate) fn get_batch_size(&self) -> u16 {
        self.events_added - self.events_emitted
    }

    pub(crate) fn process_events(&mut self) {
        self.events_emitted = self.events_added;
        self.batch_index += 1;
    }

    pub(crate) fn add_event(&mut self) {
        self.events_added += 1
    }

    pub(crate) fn has_events_to_process(&self) -> bool {
        self.batch_index == 0 || self.events_emitted < self.events_added
    }
}
