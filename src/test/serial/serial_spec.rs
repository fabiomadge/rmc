// Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.

use std::collections::VecDeque;

const LOOP_SIZE: usize = 0x40;

const DATA: u8 = 0;
const IER: u8 = 1;
const IIR: u8 = 2;
const LCR: u8 = 3;
const MCR: u8 = 4;
const LSR: u8 = 5;
const MSR: u8 = 6;
const SCR: u8 = 7;

const DLAB_LOW: u8 = 0;
const DLAB_HIGH: u8 = 1;

const IER_RECV_BIT: u8 = 0x1;
const IER_THR_BIT: u8 = 0x2;
const IER_FIFO_BITS: u8 = 0x0f;

const IIR_FIFO_BITS: u8 = 0xc0;
const IIR_NONE_BIT: u8 = 0x1;
const IIR_THR_BIT: u8 = 0x2;
const IIR_RECV_BIT: u8 = 0x4;

const LCR_DLAB_BIT: u8 = 0x80;

const LSR_DATA_BIT: u8 = 0x1;
const LSR_EMPTY_BIT: u8 = 0x20;
const LSR_IDLE_BIT: u8 = 0x40;

const MCR_LOOP_BIT: u8 = 0x10;

const DEFAULT_INTERRUPT_IDENTIFICATION: u8 = IIR_NONE_BIT; // no pending interrupt
const DEFAULT_LINE_STATUS: u8 = LSR_EMPTY_BIT | LSR_IDLE_BIT; // THR empty and line is idle
const DEFAULT_LINE_CONTROL: u8 = 0x3; // 8-bits per character
const DEFAULT_MODEM_CONTROL: u8 = 0x8; // Auxiliary output 2
const DEFAULT_MODEM_STATUS: u8 = 0x20 | 0x10 | 0x80; // data ready, clear to send, carrier detect
const DEFAULT_BAUD_DIVISOR: u16 = 12; // 9600 bps

// Cannot use multiple types as bounds for a trait object, so we define our own trait
// which is a composition of the desired bounds. In this case, io::Read and AsRawFd.
// Run `rustc --explain E0225` for more details.
// /// Trait that composes the `std::io::Read` and `std::os::unix::io::AsRawFd` traits.
// pub trait ReadableFd: io::Read + AsRawFd {}

// overwrite the following types

#[derive(Clone)]
pub struct OutWrapper {
    output: Vec<u8>,
}

impl OutWrapper {
    fn new() -> OutWrapper {
        OutWrapper { output: Vec::new() }
    }

    //write_all
    //flush

    fn write_all(&mut self, buf: &[u8]) -> Result<(), usize> {
        for b in buf {
            self.output.push(*b);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), usize> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct InputWrapper {}

impl InputWrapper {
    //read

    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, usize> {
        unimplemented!()
    }

    fn as_raw_fd(&self) -> u64 {
        unimplemented!()
    }
}

pub struct EventFd {}

impl EventFd {
    pub fn write(&self, _v: u64) -> Result<(), usize> {
        //change from io::Error to usize
        // unimplemented!()
        Ok(())
    }
}

//ghost configuration
pub struct Configuration {
    erdai: bool,
    rdai: bool,
    ethrei: bool,
    theri: bool,
    div_low: u8,
    div_high: u8,
    dlab: bool,
}

impl Configuration {
    fn new() -> Self {
        Configuration {
            erdai: true,
            rdai: false,
            ethrei: false,
            theri: false,
            div_low: 12,
            div_high: 0,
            dlab: false,
        }
    }
}

/// Emulates serial COM ports commonly seen on x86 I/O ports 0x3f8/0x2f8/0x3e8/0x2e8.
///
/// This can optionally write the guest's output to a Write trait object. To send input to the
/// guest, use `raw_input`.
pub struct Serial {
    interrupt_enable: u8,
    interrupt_identification: u8,
    interrupt_evt: EventFd,
    line_control: u8,
    line_status: u8,
    modem_control: u8,
    modem_status: u8,
    scratch: u8,
    baud_divisor: u16,
    in_buffer: VecDeque<u8>,
    out: Option<OutWrapper>,
    input: Option<InputWrapper>,
    config: Configuration,
}

impl Serial {
    fn new(interrupt_evt: EventFd, out: Option<OutWrapper>, input: Option<InputWrapper>) -> Serial {
        let mut config = Configuration::new();
        let interrupt_enable = match out {
            Some(_) => IER_RECV_BIT,
            None => {
                config.erdai = false;
                0
            }
        };
        Serial {
            interrupt_enable,
            interrupt_identification: DEFAULT_INTERRUPT_IDENTIFICATION,
            interrupt_evt,
            line_control: DEFAULT_LINE_CONTROL,
            line_status: DEFAULT_LINE_STATUS,
            modem_control: DEFAULT_MODEM_CONTROL,
            modem_status: DEFAULT_MODEM_STATUS,
            scratch: 0,
            baud_divisor: DEFAULT_BAUD_DIVISOR,
            in_buffer: VecDeque::new(),
            out,
            input,
            config,
        }
    }

    /// Constructs a Serial port ready for input and output.
    pub fn new_in_out(interrupt_evt: EventFd, input: InputWrapper, out: OutWrapper) -> Serial {
        Self::new(interrupt_evt, Some(out), Some(input))
    }

    /// Constructs a Serial port ready for output but with no input.
    pub fn new_out(interrupt_evt: EventFd, out: OutWrapper) -> Serial {
        Self::new(interrupt_evt, Some(out), None)
    }

    /// Constructs a Serial port with no connected input or output.
    pub fn new_sink(interrupt_evt: EventFd) -> Serial {
        Self::new(interrupt_evt, None, None)
    }

    /// Provides a reference to the interrupt event fd.
    pub fn interrupt_evt(&self) -> &EventFd {
        &self.interrupt_evt
    }

    fn is_dlab_set(&self) -> bool {
        (self.line_control & LCR_DLAB_BIT) != 0
    }

    fn is_recv_intr_enabled(&self) -> bool {
        (self.interrupt_enable & IER_RECV_BIT) != 0
    }

    fn is_thr_intr_enabled(&self) -> bool {
        (self.interrupt_enable & IER_THR_BIT) != 0
    }

    fn is_loop(&self) -> bool {
        (self.modem_control & MCR_LOOP_BIT) != 0
    }

    fn add_intr_bit(&mut self, bit: u8) {
        self.equiv_config();
        assert!(bit % 2 == 0);
        assert!(self.interrupt_identification & 0xf0 == 0);
        let old_dlab = self.config.dlab;

        self.interrupt_identification &= !IIR_NONE_BIT;
        self.interrupt_identification |= bit;

        self.equiv_config();
        assert!(self.config.dlab == old_dlab);
    }

    fn del_intr_bit(&mut self, bit: u8) {
        self.equiv_config();
        let old_dlab = self.config.dlab;

        self.interrupt_identification &= !bit;
        if self.interrupt_identification == 0x0 {
            self.interrupt_identification = IIR_NONE_BIT;
        }

        self.equiv_config();
        assert!(!self.config.rdai);
        assert!(!self.config.theri);
        assert!(self.config.dlab == old_dlab);
    }

    fn thr_empty_interrupt(&mut self) -> Result<(), usize> {
        self.equiv_config();
        assert!(self.interrupt_identification & 0xf0 == 0);
        let old_dlab = self.config.dlab;

        if self.is_thr_intr_enabled() {
            self.add_intr_bit(IIR_THR_BIT);
            self.interrupt_evt.write(1)?;
        }

        self.equiv_config();
        assert!(self.config.dlab == old_dlab);
        Ok(())
    }

    fn recv_data_interrupt(&mut self) -> Result<(), usize> {
        self.equiv_config();
        assert!(self.interrupt_identification & 0xf0 == 0);
        let old_dlab = self.config.dlab;

        if self.is_recv_intr_enabled() {
            self.add_intr_bit(IIR_RECV_BIT);
            self.interrupt_evt.write(1)?
        }
        self.line_status |= LSR_DATA_BIT;

        self.equiv_config();
        assert!(self.config.dlab == old_dlab);
        Ok(())
    }

    fn iir_reset(&mut self) {
        self.equiv_config();
        let old_dlab = self.config.dlab;

        self.interrupt_identification = DEFAULT_INTERRUPT_IDENTIFICATION;
        self.config.rdai = false;
        self.config.theri = false;

        self.equiv_config();
        assert!(!self.config.rdai);
        assert!(!self.config.theri);
        assert!(self.config.dlab == old_dlab);
    }

    // Handles a write request from the driver.
    fn handle_write(&mut self, offset: u8, value: u8) -> Result<(), usize> {
        self.equiv_config();
        assert!(self.interrupt_identification & 0xf0 == 0);

        match offset as u8 {
            DLAB_LOW if self.is_dlab_set() => {
                self.config.div_low = value;
                self.baud_divisor = (self.baud_divisor & 0xff00) | u16::from(value)
            }
            DLAB_HIGH if self.is_dlab_set() => {
                self.config.div_high = value;
                self.baud_divisor = (self.baud_divisor & 0x00ff) | (u16::from(value) << 8)
            }
            DATA => {
                if self.is_loop() {
                    if self.in_buffer.len() < LOOP_SIZE {
                        self.in_buffer.push_back(value);
                        self.recv_data_interrupt()?;
                    }
                } else {
                    if let Some(out) = self.out.as_mut() {
                        out.write_all(&[value])?;
                        // METRICS.uart.write_count.inc();
                        out.flush()?;
                        // METRICS.uart.flush_count.inc();
                    }
                    self.thr_empty_interrupt()?;
                }
            }
            IER => {
                self.config.erdai = value & 0x1 != 0;
                self.config.ethrei = value & 0x2 != 0;
                self.interrupt_enable = value & IER_FIFO_BITS;
            }
            LCR => {
                self.config.dlab = value & 0x80 != 0;
                self.line_control = value;
            }
            MCR => self.modem_control = value,
            SCR => self.scratch = value,
            _ => {}
        }

        self.equiv_config();
        if offset == DLAB_HIGH && !self.config.dlab {
            assert!(self.config.erdai == (value % 2 != 0));
            assert!(self.config.ethrei == ((value / 2) % 2 != 0));
        }
        if offset == LCR {
            assert!(self.config.dlab == ((value / 128) % 2 != 0));
        }
        if offset == DLAB_HIGH && self.config.dlab {
            assert!(self.config.div_high == value);
        }
        if offset == DLAB_LOW && self.config.dlab {
            assert!(self.config.div_low == value);
        }

        Ok(())
    }

    // Handles a read request from the driver.
    fn handle_read(&mut self, offset: u8) -> u8 {
        self.equiv_config();
        let old_dlab = self.config.dlab;

        let res = match offset as u8 {
            DLAB_LOW if self.is_dlab_set() => self.baud_divisor as u8,
            DLAB_HIGH if self.is_dlab_set() => (self.baud_divisor >> 8) as u8,
            DATA => {
                self.del_intr_bit(IIR_RECV_BIT);
                if self.in_buffer.len() <= 1 {
                    self.line_status &= !LSR_DATA_BIT;
                }
                // METRICS.uart.read_count.inc();
                self.in_buffer.pop_front().unwrap_or_default()
            }
            IER => self.interrupt_enable,
            IIR => {
                let v = self.interrupt_identification | IIR_FIFO_BITS;
                self.iir_reset();
                v
            }
            LCR => self.line_control,
            MCR => self.modem_control,
            LSR => self.line_status,
            MSR => self.modem_status,
            SCR => self.scratch,
            _ => 0,
        };

        self.equiv_config();
        if offset == IIR {
            assert!(!self.config.theri);
        }
        if !self.config.dlab && offset == 0 {
            assert!(!self.config.rdai);
        }
        if offset == DLAB_LOW && self.is_dlab_set() {
            assert!(res == self.baud_divisor as u8);
        }
        if offset == DLAB_HIGH && self.is_dlab_set() {
            assert!(res == (self.baud_divisor >> 8) as u8);
        }
        assert!(self.baud_divisor as u8 == self.config.div_low);
        assert!((self.baud_divisor >> 8) as u8 == self.config.div_high);

        res
    }

    fn recv_bytes(&mut self) -> Result<usize, usize> {
        if let Some(input) = self.input.as_mut() {
            let mut out = [0u8; 32];
            return input.read(&mut out).and_then(|count| {
                if count > 0 {
                    self.raw_input(&out[..count])?;
                    Ok(count)
                } else {
                    Ok(0)
                }
            });
        }

        Ok(0)
    }

    fn raw_input(&mut self, data: &[u8]) -> Result<(), usize> {
        if !self.is_loop() {
            self.in_buffer.extend(data);
            self.recv_data_interrupt()?;
        }
        Ok(())
    }

    fn read(&mut self, offset: u64, data: &mut [u8]) {
        if data.len() != 1 {
            // METRICS.uart.missed_read_count.inc();
            return;
        }

        data[0] = self.handle_read(offset as u8);
    }

    fn write(&mut self, offset: u64, data: &[u8]) {
        if data.len() != 1 {
            // METRICS.uart.missed_write_count.inc();
            return;
        }
        if let Err(_e) = self.handle_write(offset as u8, data[0]) {
            // error!("Failed the write to serial: {}", e);
            // METRICS.uart.error_count.inc();
            assert!(false)
        }
    }

    pub fn equiv_config(&self) {
        let dlab = self.is_dlab_set();

        let erdai = self.is_recv_intr_enabled();

        let rdai = self.interrupt_identification & IIR_RECV_BIT != 0 &&//0x04;
            self.interrupt_identification & IIR_NONE_BIT == 0; //pending

        let ethrei = self.interrupt_enable & IIR_THR_BIT != 0; //0x02;

        let theri = self.interrupt_identification & IIR_THR_BIT != 0 &&//0x02
            self.interrupt_identification & IIR_NONE_BIT == 0; //pending

        assert!(self.config.dlab == dlab);
        assert!(self.config.erdai == erdai);
        assert!(self.config.rdai == rdai);
        assert!(self.config.ethrei == ethrei);
        assert!(self.config.theri == theri);
        assert!(self.config.div_low == (self.baud_divisor & 0xff) as u8);
        assert!(self.config.div_high == (self.baud_divisor >> 8) as u8);
    }
}

fn main() {
    {
        let mut serial = Serial::new_sink(EventFd {});
        let a: u8 = rmc::any();
        let b: u8 = rmc::any();
        let c: u8 = rmc::any();

        serial.write(MCR as u64, &[MCR_LOOP_BIT as u8]);
        serial.write(DATA as u64, &[a]);
        serial.write(DATA as u64, &[b]);
        serial.write(DATA as u64, &[c]);

        let mut data = [0u8];
        serial.read(MSR as u64, &mut data[..]);
        assert!(data[0] == DEFAULT_MODEM_STATUS as u8);
        serial.read(MCR as u64, &mut data[..]);
        assert!(data[0] == MCR_LOOP_BIT as u8);
        serial.read(DATA as u64, &mut data[..]);
        assert!(data[0] == a);
        serial.read(DATA as u64, &mut data[..]);
        assert!(data[0] == b);
        serial.read(DATA as u64, &mut data[..]);
        assert!(data[0] == c);
    }
}
