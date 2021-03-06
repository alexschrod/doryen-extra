/* BSD 3-Clause License
 *
 * Copyright © 2019, Alexander Krivács Schrøder <alexschrod@gmail.com>.
 * Copyright © 2008-2019, Jice and the libtcod contributors.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * 1. Redistributions of source code must retain the above copyright notice,
 *    this list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright notice,
 *    this list of conditions and the following disclaimer in the documentation
 *    and/or other materials provided with the distribution.
 *
 * 3. Neither the name of the copyright holder nor the names of its
 *    contributors may be used to endorse or promote products derived from
 *    this software without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 * AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
 * LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

//! Random number generator algorithms.

use std::mem::{transmute, MaybeUninit};

const RAND_DIV: f32 = 1.0 / 0xffff_ffff_u32 as f32; // u32::MAX
#[allow(clippy::unnecessary_cast)]
const RAND_DIV_DOUBLE: f64 = 1.0 / 0xffff_ffff_u32 as f64; // u32::MAX

/// Random number generator algorithm trait.
pub trait Algorithm {
    /// Generate a 32-bit integer.
    fn get_int(&mut self) -> u32;

    /// Generate a 32-bit floating point number.
    fn get_float(&mut self) -> f32 {
        if cfg!(feature = "libtcod-compat") {
            self.get_int() as f32 * RAND_DIV
        } else {
            // Generate a random 32-bit floating point number between 0 and 1
            // using the Allen Downey algorithm described here:
            // <https://allendowney.com/research/rand/downey07randfloat.pdf>
            //
            // The desirable properties of this algorithm is that it can produce
            // every representable floating-point value in a given range
            // (as opposed to about 7% of them with the "libtcod-compat" method
            // above) and it produces uniformly distributed values.
            //
            // Thanks to <https://github.com/orlp> for the link to the article.

            let low_exp = 0;
            let high_exp = 127;

            let mut bits = Bits::new(self);
            let mut exp = high_exp - 1;
            while exp > low_exp {
                if bits.get_bit() != 0 {
                    break;
                }
                exp -= 1;
            }

            let mantissa = bits.algorithm.get_int() & 0x7FFFFF;
            if mantissa == 0 && bits.get_bit() != 0 {
                exp += 1;
            }

            let ans = (exp << 23) | mantissa;

            f32::from_bits(ans)
        }
    }

    /// Generate a 64-bit floating point number.
    fn get_double(&mut self) -> f64 {
        if cfg!(feature = "libtcod-compat") {
            f64::from(self.get_int()) * RAND_DIV_DOUBLE
        } else {
            // Generate a random 64-bit floating point number between 0 and 1
            // using the Allen Downey algorithm described here:
            // <https://allendowney.com/research/rand/downey07randfloat.pdf>
            //
            // The desirable properties of this algorithm is that it can produce
            // every representable floating-point value in a given range
            // (as opposed to a minuscule amount of them with the
            // "libtcod-compat" method above) and it produces uniformly
            // distributed values.
            //
            // Thanks to <https://github.com/orlp> for the link to the article.

            let low_exp = 0;
            let high_exp = 1023;

            let mut bits = Bits::new(self);
            let mut exp = high_exp - 1;
            while exp > low_exp {
                if bits.get_bit() != 0 {
                    break;
                }
                exp -= 1;
            }

            let mantissa = (u64::from(bits.algorithm.get_int()) << 32
                | u64::from(bits.algorithm.get_int()))
                & 0xFFFFFFFFFFFFF;
            if mantissa == 0 && bits.get_bit() != 0 {
                exp += 1;
            }

            let ans = (exp << 52) | mantissa;

            f64::from_bits(ans)
        }
    }
}

/// Mersenne Twister algorithm.
#[derive(Clone, Copy)]
pub struct MersenneTwister {
    mt: [u32; Self::MT19937_RECURRENCE_DEGREE],
    cur_mt: usize,
}

impl MersenneTwister {
    const MT19937: u32 = 1_812_433_253;
    const MT19937_WORD_SIZE: usize = 32;
    const MT19937_RECURRENCE_DEGREE: usize = 624;
    const MT19937_SEPARATION_POINT: usize = 31;
    const MT19937_MIDDLE_WORD: usize = 397;
    const MT19937_RATIONAL_NORMAL_FORM_TWIST_MATRIX_COEFFICIENTS: u32 = 0x9908_B0DF;
    const MT19937_TGFSR_R_TEMPERING_BIT_MASKS: (u32, u32) = (0x9D2C_5680, 0xEFC6_0000);
    const MT19937_TGFSR_R_TEMPERING_BIT_SHIFTS: (u32, u32) = (7, 15);
    const MT19937_ADDITIONAL_TEMPERING: (u32, u32, u32) = (11, 0xFFFF_FFFF, 18);
    const MT19937_LOWER_MASK: u32 = (1 << (Self::MT19937_SEPARATION_POINT)) as u32;
    const MT19937_UPPER_MASK: u32 = !Self::MT19937_LOWER_MASK;

    /// Create a new Mersenne Twister algorithm instance.
    pub fn new(seed: u32) -> Self {
        Self {
            cur_mt: 624,
            mt: Self::mt_init(seed),
        }
    }

    /* initialize the mersenne twister array */
    #[allow(unsafe_code)]
    fn mt_init(seed: u32) -> [u32; Self::MT19937_RECURRENCE_DEGREE] {
        let mut mt: [MaybeUninit<u32>; Self::MT19937_RECURRENCE_DEGREE] =
            unsafe { MaybeUninit::uninit().assume_init() };
        mt[0] = MaybeUninit::new(seed);
        for i in 1..mt.len() {
            mt[i] = MaybeUninit::new(unsafe {
                Self::MT19937.wrapping_mul(
                    (*mt[i - 1].as_ptr()
                        ^ (*mt[i - 1].as_ptr() >> (Self::MT19937_WORD_SIZE as u32 - 2)))
                        .wrapping_add(i as u32),
                )
            });
        }

        unsafe { transmute(mt) }
    }

    /* get the next random value from the mersenne twister array */
    fn mt_rand(mt: &mut [u32; Self::MT19937_RECURRENCE_DEGREE], cur_mt: &mut usize) -> u32 {
        if *cur_mt == Self::MT19937_RECURRENCE_DEGREE {
            /* our 624 sequence is finished. generate a new one */
            for i in 0..Self::MT19937_RECURRENCE_DEGREE - 1 {
                let y = (mt[i] & Self::MT19937_LOWER_MASK) | (mt[i + 1] & Self::MT19937_UPPER_MASK);
                if y & 1 == 0 {
                    /* even y */
                    mt[i] = mt[(i + Self::MT19937_MIDDLE_WORD) % Self::MT19937_RECURRENCE_DEGREE]
                        ^ (y >> 1);
                } else {
                    /* odd y */
                    mt[i] = mt[(i + Self::MT19937_MIDDLE_WORD) % Self::MT19937_RECURRENCE_DEGREE]
                        ^ (y >> 1)
                        ^ Self::MT19937_RATIONAL_NORMAL_FORM_TWIST_MATRIX_COEFFICIENTS;
                }
            }

            let y = (mt[Self::MT19937_RECURRENCE_DEGREE - 1] & Self::MT19937_LOWER_MASK)
                | (mt[0] & Self::MT19937_UPPER_MASK);
            if y & 1 == 0 {
                /* even y */
                mt[Self::MT19937_RECURRENCE_DEGREE - 1] =
                    mt[Self::MT19937_MIDDLE_WORD - 1] ^ (y >> 1);
            } else {
                /* odd y */
                mt[Self::MT19937_RECURRENCE_DEGREE - 1] = mt[Self::MT19937_MIDDLE_WORD - 1]
                    ^ (y >> 1)
                    ^ Self::MT19937_RATIONAL_NORMAL_FORM_TWIST_MATRIX_COEFFICIENTS;
            }

            *cur_mt = 0;
        }

        let mut y = mt[*cur_mt];
        *cur_mt += 1;

        y ^= y >> Self::MT19937_ADDITIONAL_TEMPERING.0;
        y ^= (y << Self::MT19937_TGFSR_R_TEMPERING_BIT_SHIFTS.0)
            & Self::MT19937_TGFSR_R_TEMPERING_BIT_MASKS.0;
        y ^= (y << Self::MT19937_TGFSR_R_TEMPERING_BIT_SHIFTS.1)
            & Self::MT19937_TGFSR_R_TEMPERING_BIT_MASKS.1;
        y ^= y >> Self::MT19937_ADDITIONAL_TEMPERING.2;

        y
    }
}

impl std::fmt::Debug for MersenneTwister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "MersenneTwister {{ cur_mt: {} }}", self.cur_mt)
    }
}

impl Algorithm for MersenneTwister {
    fn get_int(&mut self) -> u32 {
        Self::mt_rand(&mut self.mt, &mut self.cur_mt)
    }
}

/// Complementary-Multiply-With-Carry algorithm.
#[derive(Clone, Copy)]
pub struct ComplementaryMultiplyWithCarry {
    q: [u32; 4096],
    c: u32,
    cur: usize,
}

impl ComplementaryMultiplyWithCarry {
    /// Create a new Complementary-Multiply-With-Carry algorithm instance.
    #[allow(unsafe_code)]
    pub fn new(seed: u32) -> Self {
        let mut s = seed;
        let mut q: [MaybeUninit<u32>; 4096] = unsafe { MaybeUninit::uninit().assume_init() };
        for qe in &mut q[..] {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345); /* glibc LCG */
            unsafe {
                qe.as_mut_ptr().write(s);
            }
        }
        let c = s.wrapping_mul(1_103_515_245).wrapping_add(12345) % 809_430_660; /* this max value is recommended by George Marsaglia */
        let cur = 0;

        Self {
            q: unsafe { transmute(q) },
            c,
            cur,
        }
    }

    fn get_number(&mut self) -> u32 {
        self.cur = (self.cur + 1) & 4095;
        let t = 18782_u64 * u64::from(self.q[self.cur]) + u64::from(self.c);
        self.c = (t >> 32) as u32;
        let mut x = (t + u64::from(self.c)) as u32;
        if x < self.c {
            x += 1;
            self.c += 1;
        }
        if x.wrapping_add(1) == 0 {
            self.c += 1;
            x = 0;
        }
        self.q[self.cur] = 0xffff_fffe - x;

        self.q[self.cur]
    }
}

impl std::fmt::Debug for ComplementaryMultiplyWithCarry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "ComplementaryMultiplyWithCarry {{ c: {}, cur: {} }}",
            self.c, self.cur
        )
    }
}

impl Algorithm for ComplementaryMultiplyWithCarry {
    fn get_int(&mut self) -> u32 {
        self.get_number()
    }
}

struct Bits<'a, A: Algorithm + ?Sized> {
    algorithm: &'a mut A,
    bits: u32,
    bits_left: u32,
}

impl<'a, A: Algorithm + ?Sized> Bits<'a, A> {
    fn new(algorithm: &'a mut A) -> Self {
        Self {
            algorithm,
            bits: 0,
            bits_left: 0,
        }
    }

    fn get_bit(&mut self) -> u32 {
        if self.bits_left == 0 {
            self.bits = self.algorithm.get_int();
            self.bits_left = 32;
        }

        let bit = self.bits & 1;
        self.bits >>= 1;
        self.bits_left -= 1;
        bit
    }
}
