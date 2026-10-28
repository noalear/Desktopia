//! Inheritance. Every creature carries two sets of genes (haplotypes), one
//! from each parent, laid out along six chromosomes. A parent passes on one
//! gamete: a single set shuffled from its own two by crossing over, so genes
//! that sit close together tend to travel together. What a creature is comes
//! from both copies at once: each allele has its own strength of expression,
//! so some are dominant, some recessive and some blend.
//!
//! Three things make the genes more than a list of numbers:
//!
//! - Harmful mutations. Most new mutations that break a gene are recessive:
//!   carried quietly in one copy, they only hurt when both copies are broken.
//!   Inbreeding and selfing make that likely (inbreeding depression); without
//!   sex they can only pile up (Muller's ratchet).
//! - Genes that must work together. Each lineage fixes its own alleles at a
//!   set of interacting genes, and any two alleles that never met before may
//!   clash. Two lineages long apart carry more and more clashing pairs, so
//!   their hybrids fail more and more often (Dobzhansky-Muller), first by
//!   being sterile and then by not developing at all. This is what ends up
//!   making species, rather than a label.
//! - Immune genes. Both copies are expressed, so a host with two different
//!   alleles fends off more strains, and rare alleles are worth having.

use crate::genome::{BrainGenes, Genome, MAX_PARTS, PartGene};
use crate::rng::{Rng, hash01};
use std::sync::Arc;

/// Quantitative loci, in chromosome order (see `loci_mut`).
pub const LOCI: usize = 45;
/// Genes that must work with each other.
pub const COMPAT: usize = 16;
pub const CHROMOSOMES: usize = 6;
/// The first locus on each chromosome; chromosome 5 carries the body's part genes.
const CHROM_START: [usize; CHROMOSOMES] = [0, 9, 15, 21, 41, LOCI];
/// Loci that are hues, which blend around the colour wheel.
const CIRCULAR: [usize; 4] = [21, 22, 27, 28];
/// Chromosome that carries genes the locus list doesn't know by name.
const UNLISTED: usize = 2;
const BODY: usize = 5;

/// Harmful mutations per gamete at mutation rate 1.
const HARM_RATE: f32 = 0.05;
/// Fitness cost of a gene broken in both copies, and how much of it one broken copy shows.
const HARM: f32 = 0.08;
const HARM_DOMINANCE: f32 = 0.12;
/// Genes that can't be lost: broken in both copies, the embryo dies.
const ESSENTIAL: u64 = 0xFF;
/// Chance per gamete, per interacting gene, of a new allele.
const COMPAT_RATE: f32 = 0.01;
/// Chance that any two alleles that never met before clash.
const CLASH: f32 = 0.03;
/// Survival and fertility left by each clashing pair.
const CLASH_VIABILITY: f32 = 0.6;
const CLASH_FERTILITY: f32 = 0.45;

/// A mutation's size, in standard deviations: mostly small, now and then large.
pub fn effect(rng: &mut Rng) -> f32 {
    if rng.chance(0.06) { rng.normal() * 5.0 } else { rng.normal() }
}

/// The named loci's values, in the same order as `loci_mut`.
pub(crate) fn loci(g: &Genome) -> [f32; LOCI] {
    let (l, p, i) = (&g.look, &g.mate_pref, &g.immune);
    [
        g.adult_scale,
        g.growth_time,
        g.aging,
        g.gait_hz,
        g.invest,
        g.clutch,
        g.live_birth,
        g.helpers,
        g.matriphagy,
        g.diet,
        g.heat_tolerance,
        g.cold_tolerance,
        g.toxin,
        g.mutation,
        g.canalization,
        g.sexual,
        g.selfing,
        g.choosiness,
        g.mhc_pref,
        g.imitation,
        g.innovation,
        l[0],
        p[0],
        l[1],
        p[1],
        l[2],
        p[2],
        l[3],
        p[3],
        l[4],
        l[5],
        l[6],
        l[7],
        l[8],
        l[9],
        l[10],
        l[11],
        l[12],
        l[13],
        l[14],
        l[15],
        i[0],
        i[1],
        i[2],
        i[3],
    ]
}

/// The named quantitative loci. Order is position along the chromosomes:
/// life history; body chemistry; mating and learning; looks interleaved with
/// the matching mate preference, so that a taste and the trait it favours
/// are inherited together; and the immune block.
pub(crate) fn loci_mut(g: &mut Genome) -> [&mut f32; LOCI] {
    let [l0, l1, l2, l3, l4, l5, l6, l7, l8, l9, l10, l11, l12, l13, l14, l15] = &mut g.look;
    let [p0, p1, p2, p3] = &mut g.mate_pref;
    let [i0, i1, i2, i3] = &mut g.immune;
    [
        &mut g.adult_scale,
        &mut g.growth_time,
        &mut g.aging,
        &mut g.gait_hz,
        &mut g.invest,
        &mut g.clutch,
        &mut g.live_birth,
        &mut g.helpers,
        &mut g.matriphagy,
        &mut g.diet,
        &mut g.heat_tolerance,
        &mut g.cold_tolerance,
        &mut g.toxin,
        &mut g.mutation,
        &mut g.canalization,
        &mut g.sexual,
        &mut g.selfing,
        &mut g.choosiness,
        &mut g.mhc_pref,
        &mut g.imitation,
        &mut g.innovation,
        l0,
        p0,
        l1,
        p1,
        l2,
        p2,
        l3,
        p3,
        l4,
        l5,
        l6,
        l7,
        l8,
        l9,
        l10,
        l11,
        l12,
        l13,
        l14,
        l15,
        i0,
        i1,
        i2,
        i3,
    ]
}

/// Chromosome and position (0..1) of a named locus.
fn place(i: usize) -> (usize, f32) {
    let c = CHROM_START.iter().rposition(|&s| s <= i).unwrap_or(0).min(BODY - 1);
    let end = CHROM_START[c + 1];
    (c, (i - CHROM_START[c]) as f32 / (end - CHROM_START[c]) as f32 + 0.5 / (end - CHROM_START[c]) as f32)
}

fn place_compat(k: usize) -> (usize, f32) {
    (k % BODY, ((k / BODY) as f32 + 0.5) / COMPAT.div_ceil(BODY) as f32)
}

fn place_load(bit: usize) -> (usize, f32) {
    (bit % CHROMOSOMES, ((bit / CHROMOSOMES) as f32 + 0.5) / 64usize.div_ceil(CHROMOSOMES) as f32)
}

fn place_part(i: usize) -> (usize, f32) {
    (BODY, (i as f32 + 0.5) / MAX_PARTS as f32)
}

/// Haplotypes don't carry wiring of their own.
fn bare(b: &mut BrainGenes) {
    b.hidden = 0;
    for v in [&mut b.w_ih, &mut b.w_hh, &mut b.w_ho, &mut b.w_io, &mut b.b_h, &mut b.b_o] {
        *v = Vec::new();
    }
}

/// Weight an allele gets in expression from its dominance.
fn weight(d: Option<&u8>) -> f32 {
    ((*d.unwrap_or(&128) as f32 - 128.0) / 24.0).exp2()
}

/// One round of crossing over: which of the two strands each stretch of
/// each chromosome comes from. Every chromosome crosses over at least once.
struct Meiosis {
    start: [u8; CHROMOSOMES],
    cuts: [[f32; 3]; CHROMOSOMES],
    n: [u8; CHROMOSOMES],
}

impl Meiosis {
    fn new(rng: &mut Rng) -> Self {
        let mut m = Self { start: [0; CHROMOSOMES], cuts: [[0.0; 3]; CHROMOSOMES], n: [0; CHROMOSOMES] };
        for c in 0..CHROMOSOMES {
            m.start[c] = rng.chance(0.5) as u8;
            let u = rng.f32();
            m.n[c] = if u < 0.6 { 1 } else if u < 0.9 { 2 } else { 3 };
            for k in 0..m.n[c] as usize {
                m.cuts[c][k] = rng.f32();
            }
        }
        m
    }

    fn strand(&self, c: usize, pos: f32) -> usize {
        let k = self.cuts[c][..self.n[c] as usize].iter().filter(|&&x| x < pos).count() as u8;
        ((self.start[c] + k) & 1) as usize
    }
}

/// Builds one haplotype from two, taking each gene from the strand `pick` names.
fn assemble(src: [&Genome; 2], pick: impl Fn(usize, f32) -> usize) -> Genome {
    let mut out = src[pick(UNLISTED, 0.5)].clone_shallow();
    let (a, b) = (loci(src[0]), loci(src[1]));
    for (i, slot) in loci_mut(&mut out).into_iter().enumerate() {
        let (c, pos) = place(i);
        *slot = if pick(c, pos) == 0 { a[i] } else { b[i] };
    }
    out.dominance = (0..LOCI)
        .map(|i| {
            let (c, pos) = place(i);
            src[pick(c, pos)].dominance.get(i).copied().unwrap_or(128)
        })
        .collect();
    let n = src[0].parts.len().max(src[1].parts.len());
    out.parts = (0..n)
        .filter_map(|i| {
            let (c, pos) = place_part(i);
            let s = pick(c, pos);
            src[s].parts.get(i).or(src[1 - s].parts.get(i)).cloned()
        })
        .collect();
    for k in 0..COMPAT {
        let (c, pos) = place_compat(k);
        out.compat[k] = src[pick(c, pos)].compat[k];
    }
    out.load = 0;
    for bit in 0..64 {
        let (c, pos) = place_load(bit);
        out.load |= src[pick(c, pos)].load & (1 << bit);
    }
    out
}

/// A gamete from two haplotypes.
pub(crate) fn recombine(a: &Genome, b: &Genome, rng: &mut Rng) -> Genome {
    let m = Meiosis::new(rng);
    assemble([a, b], |c, pos| m.strand(c, pos))
}

/// A gamete from a creature's own two haplotypes.
pub(crate) fn gamete(g: &Genome, rng: &mut Rng) -> Genome {
    let [a, b] = haplotypes(g);
    recombine(&a, &b, rng)
}

/// A creature's two haplotypes. A genome without them (an old record) is
/// read as carrying two identical copies of what it expresses.
pub(crate) fn haplotypes(g: &Genome) -> [Genome; 2] {
    match &g.haplotypes {
        Some(h) => [h[0].clone(), h[1].clone()],
        None => {
            let h = haplotype_of(g);
            [h.clone(), h]
        }
    }
}

/// Copies of both haplotypes for an offspring without sex. Now and then a
/// chromosome is copied over its partner, as happens in clonal lineages, so
/// that even they slowly lose what made them heterozygous.
pub(crate) fn clone_haplotypes(g: &Genome, rng: &mut Rng) -> [Genome; 2] {
    let [mut a, mut b] = haplotypes(g);
    for c in 0..CHROMOSOMES {
        if rng.chance(0.02) {
            let keep_a = rng.chance(0.5);
            let merged = assemble([&a, &b], |k, _| if k == c { if keep_a { 0 } else { 1 } } else { 1 });
            if keep_a {
                b = merged;
            } else {
                // The same chromosome, the other way round.
                a = assemble([&a, &b], |k, _| if k == c { 1 } else { 0 });
            }
        }
    }
    [a, b]
}

/// An expressed genome as a haplotype: no brain of its own (the brain is
/// inherited as a block alongside), and no haplotypes inside.
fn haplotype_of(g: &Genome) -> Genome {
    let mut h = g.clone_shallow();
    bare(&mut h.brain);
    if h.dominance.len() != LOCI {
        h.dominance = vec![128; LOCI];
    }
    h
}

/// Makes a designed founder genome into a living one: two haplotypes that
/// differ a little, with alleles at the interacting genes that all work
/// together, and two different immune alleles.
pub(crate) fn found(plan: Genome, rng: &mut Rng) -> Genome {
    let mut a = haplotype_of(&plan);
    for d in a.dominance.iter_mut() {
        *d = (128.0 + rng.normal() * 20.0).clamp(1.0, 255.0) as u8;
    }
    a.load = 0;
    for k in 0..COMPAT {
        // Draw until it gets on with the alleles already chosen.
        for _ in 0..64 {
            a.compat[k] = rng.below(256) as u8;
            if (0..k).all(|j| !clash(j, a.compat[j], k, a.compat[k])) {
                break;
            }
        }
    }
    let mut b = a.clone();
    b.mutate_genes(rng, 1.0);
    for d in b.dominance.iter_mut() {
        *d = (128.0 + rng.normal() * 20.0).clamp(1.0, 255.0) as u8;
    }
    for v in b.immune.iter_mut() {
        *v = rng.f32();
    }
    let brain = plan.brain.clone();
    let marker = plan.marker;
    express([a, b], brain, marker, rng)
}

/// Copying errors as a gamete is made: in the genes themselves, in how
/// strongly they are expressed, harmful breakages, and new alleles at the
/// interacting genes.
pub(crate) fn mutate_haplotype(h: &mut Genome, rng: &mut Rng, rate: f32) {
    let r = rate.clamp(0.2, 3.0);
    h.mutate_genes(rng, rate);
    if h.dominance.len() != LOCI {
        h.dominance.resize(LOCI, 128);
    }
    for d in h.dominance.iter_mut() {
        if rng.chance(0.015 * r) {
            *d = (*d as f32 + rng.normal() * 24.0).clamp(1.0, 255.0) as u8;
        }
    }
    let mut harms = HARM_RATE * r;
    while harms > 0.0 {
        if rng.chance(harms.min(1.0)) {
            h.load |= 1 << rng.below(64);
        }
        harms -= 1.0;
    }
    // Rarely a broken gene is mended by a second mutation.
    if h.load != 0 && rng.chance(0.002 * r) {
        h.load &= !(1 << rng.below(64));
    }
    for k in 0..COMPAT {
        if rng.chance(COMPAT_RATE * r) {
            h.compat[k] = rng.below(256) as u8;
        }
    }
}

/// The mother line passes on whole, but for copying errors.
pub(crate) fn mother_line(marker: &[u8; 8], rng: &mut Rng) -> [u8; 8] {
    let mut m = *marker;
    for b in m.iter_mut() {
        if rng.chance(0.03) {
            *b = rng.below(256) as u8;
        }
    }
    m
}

/// What two haplotypes make together. The maternal haplotype is the
/// starting point, so any gene not listed in `loci_mut` follows the mother.
pub(crate) fn express(h: [Genome; 2], brain: BrainGenes, marker: [u8; 8], rng: &mut Rng) -> Genome {
    let mut g = h[0].clone_shallow();
    let (a, b) = (loci(&h[0]), loci(&h[1]));
    for (i, slot) in loci_mut(&mut g).into_iter().enumerate() {
        let (wa, wb) = (weight(h[0].dominance.get(i)), weight(h[1].dominance.get(i)));
        let t = wb / (wa + wb);
        *slot = if CIRCULAR.contains(&i) {
            let d = (b[i] - a[i] + 0.5).rem_euclid(1.0) - 0.5;
            (a[i] + d * t).rem_euclid(1.0)
        } else {
            a[i] + (b[i] - a[i]) * t
        };
    }
    // Both immune alleles work side by side.
    g.immune = h[0].immune;
    g.immune_b = h[1].immune;
    g.parts = express_parts(&h[0].parts, &h[1].parts);
    g.brain = brain;
    g.marker = marker;
    g.dominance = Vec::new();
    // What it carries broken in both copies.
    g.load = h[0].load & h[1].load;
    let hom = (h[0].load & h[1].load).count_ones() as i32;
    let het = (h[0].load ^ h[1].load).count_ones() as i32;
    g.vigor = (1.0 - HARM).powi(hom) * (1.0 - HARM * HARM_DOMINANCE).powi(het);
    // A worse-built body ages faster, grows slower and breeds smaller clutches.
    let v = g.vigor.max(0.05);
    g.aging /= v.sqrt();
    g.growth_time /= v.sqrt();
    g.clutch = (g.clutch * v).max(1.0);
    develop_noise(&mut g, rng);
    g.haplotypes = Some(Arc::new(h));
    g
}

/// No body grows exactly as its genes say: each part comes out a little
/// longer or shorter, thicker or thinner, set at a slightly different angle,
/// and the patterns on the skin fall a little differently. Canalized
/// genomes buffer it; broken genes make it worse.
fn develop_noise(g: &mut Genome, rng: &mut Rng) {
    let sd = 0.07 * (1.0 - g.canalization.clamp(0.0, 1.0)) + 0.15 * (1.0 - g.vigor);
    for p in g.parts.iter_mut() {
        p.length = (p.length * (1.0 + rng.normal() * sd)).max(0.05);
        p.width = (p.width * (1.0 + rng.normal() * sd)).max(0.02);
        p.taper = (p.taper * (1.0 + rng.normal() * sd)).clamp(0.1, 1.6);
        for l in p.links.iter_mut() {
            l.angle += rng.normal() * sd * 0.6;
            l.scale = (l.scale * (1.0 + rng.normal() * sd)).max(0.1);
        }
    }
    for v in g.look.iter_mut().skip(4) {
        *v = (*v + rng.normal() * sd * 0.5).clamp(0.0, 1.0);
    }
}

/// The body plan two haplotypes make. Part genes that descend from the same
/// ancestral gene blend, the stronger copy deciding the tissue and wiring;
/// where the two differ in kind, the stronger one wins outright.
fn express_parts(a: &[PartGene], b: &[PartGene]) -> Vec<PartGene> {
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| match (a.get(i), b.get(i)) {
            (Some(p), Some(q)) if p.origin == q.origin => blend_part(p, q),
            (Some(p), Some(q)) => if p.dom >= q.dom { p } else { q }.clone(),
            (Some(p), None) | (None, Some(p)) => p.clone(),
            (None, None) => unreachable!(),
        })
        .collect()
}

fn blend_part(p: &PartGene, q: &PartGene) -> PartGene {
    let (d, r) = if p.dom >= q.dom { (p, q) } else { (q, p) };
    let (wd, wr) = (weight(Some(&d.dom)), weight(Some(&r.dom)));
    let t = wr / (wd + wr);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let turn = |x: f32, y: f32| (x + ((y - x + 0.5).rem_euclid(1.0) - 0.5) * t).rem_euclid(1.0);
    let mut out = d.clone();
    out.length = mix(d.length, r.length);
    out.width = mix(d.width, r.width);
    out.taper = mix(d.taper, r.taper);
    for (k, l) in out.links.iter_mut().enumerate() {
        if let Some(m) = r.links.get(k).filter(|m| m.to == l.to) {
            l.angle = mix(l.angle, m.angle);
            l.along = mix(l.along, m.along);
            l.scale = mix(l.scale, m.scale);
            l.onset = mix(l.onset, m.onset);
            l.motor.amp = mix(l.motor.amp, m.motor.amp);
            l.motor.phase = turn(l.motor.phase, m.motor.phase);
            l.motor.pair = turn(l.motor.pair, m.motor.pair);
            l.motor.lift = mix(l.motor.lift, m.motor.lift);
        }
    }
    out
}

/// Whether two alleles at two interacting genes clash. Fixed for all time,
/// as chemistry is: the same pair always clashes or always works.
fn clash(i: usize, x: u8, j: usize, y: u8) -> bool {
    let (i, x, j, y) = if i < j { (i, x, j, y) } else { (j, y, i, x) };
    hash01(0xD0B2_4A11_0000_0000 ^ (i as u64) << 24 ^ (x as u64) << 16 ^ (j as u64) << 8 ^ y as u64) < CLASH
}

/// Clashing pairs of alleles a creature carries.
pub fn clashes(g: &Genome) -> u32 {
    let Some(h) = &g.haplotypes else { return 0 };
    let (a, b) = (&h[0].compat, &h[1].compat);
    let mut n = 0;
    for i in 0..COMPAT {
        for j in i + 1..COMPAT {
            let mut seen: [(u8, u8); 4] = [(0, 0); 4];
            let mut k = 0;
            for x in [a[i], b[i]] {
                for y in [a[j], b[j]] {
                    if seen[..k].contains(&(x, y)) {
                        continue;
                    }
                    seen[k] = (x, y);
                    k += 1;
                    n += clash(i, x, j, y) as u32;
                }
            }
        }
    }
    n
}

/// Chance an embryo with these genes develops at all.
pub fn viability(g: &Genome) -> f32 {
    if let Some(h) = &g.haplotypes
        && h[0].load & h[1].load & ESSENTIAL != 0
    {
        return 0.0;
    }
    CLASH_VIABILITY.powi(clashes(g) as i32)
}

/// Chance it can breed once grown.
pub fn fertility(g: &Genome) -> f32 {
    CLASH_FERTILITY.powi(clashes(g) as i32)
}

/// How alike two creatures' immune genes are, 0 nothing shared .. 1 the same.
pub fn immune_likeness(a: &Genome, b: &Genome) -> f32 {
    let d = |x: &[f32; 4], y: &[f32; 4]| x.iter().zip(y).map(|(p, q)| (p - q).abs()).sum::<f32>() / 4.0;
    let best = |x: &[f32; 4]| d(x, &b.immune).min(d(x, &b.immune_b));
    (1.0 - (best(&a.immune) + best(&a.immune_b)) * 2.0).clamp(0.0, 1.0)
}

/// Fraction of its genes at which its two copies differ, 0..1.
pub fn heterozygosity(g: &Genome) -> f32 {
    let Some(h) = &g.haplotypes else { return 0.0 };
    let (a, b) = (loci(&h[0]), loci(&h[1]));
    let mut differ = a.iter().zip(&b).filter(|(x, y)| (*x - *y).abs() > 1e-6).count();
    differ += h[0].compat.iter().zip(&h[1].compat).filter(|(x, y)| x != y).count();
    differ as f32 / (LOCI + COMPAT) as f32
}

/// Harmful mutations carried in one copy, and in both.
pub fn load(g: &Genome) -> (u32, u32) {
    let Some(h) = &g.haplotypes else { return (0, 0) };
    ((h[0].load ^ h[1].load).count_ones(), (h[0].load & h[1].load).count_ones())
}

/// How far apart two creatures' interacting genes are, 0 same alleles .. 1 none shared.
pub fn compat_divergence(a: &Genome, b: &Genome) -> f32 {
    let (Some(ha), Some(hb)) = (&a.haplotypes, &b.haplotypes) else { return 0.0 };
    let mut apart = 0;
    for k in 0..COMPAT {
        let x = [ha[0].compat[k], ha[1].compat[k]];
        let y = [hb[0].compat[k], hb[1].compat[k]];
        if !x.iter().any(|v| y.contains(v)) {
            apart += 1;
        }
    }
    apart as f32 / COMPAT as f32
}

impl Genome {
    /// A copy without the haplotypes it was expressed from, for records.
    pub fn clone_shallow(&self) -> Genome {
        let mut g = self.clone();
        g.haplotypes = None;
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Founder;

    fn founder(seed: u64, kind: Founder) -> (Genome, Rng) {
        let mut rng = Rng::new(seed);
        let g = Genome::founder(kind, &mut rng);
        (g, rng)
    }

    #[test]
    fn loci_read_and_write_the_same_genes() {
        let (mut g, _) = founder(1, Founder::Grazer);
        let read = loci(&g);
        let written: Vec<f32> = loci_mut(&mut g).into_iter().map(|v| *v).collect();
        assert_eq!(read.to_vec(), written);
        assert_eq!(CHROM_START[BODY - 1] + 4, LOCI);
    }

    #[test]
    fn alleles_segregate_half_and_half() {
        let (g, mut rng) = founder(2, Founder::Grazer);
        let [mut a, mut b] = haplotypes(&g);
        a.diet = 0.1;
        b.diet = 0.9;
        let n = 4000;
        let from_a = (0..n).filter(|_| recombine(&a, &b, &mut rng).diet == 0.1).count();
        let f = from_a as f32 / n as f32;
        assert!((f - 0.5).abs() < 0.03, "{f}");
    }

    #[test]
    fn neighbouring_genes_travel_together() {
        let (g, mut rng) = founder(3, Founder::Grazer);
        let [mut a, mut b] = haplotypes(&g);
        // Two neighbours on chromosome 1, and one on chromosome 2.
        (a.diet, a.heat_tolerance, a.sexual) = (0.0, 0.0, 0.0);
        (b.diet, b.heat_tolerance, b.sexual) = (1.0, 1.0, 1.0);
        let n = 4000;
        let (mut near, mut far) = (0, 0);
        for _ in 0..n {
            let h = recombine(&a, &b, &mut rng);
            near += (h.diet == h.heat_tolerance) as u32;
            far += (h.diet == h.sexual) as u32;
        }
        let (near, far) = (near as f32 / n as f32, far as f32 / n as f32);
        assert!(near > 0.65, "linked genes stayed together {near}");
        assert!((far - 0.5).abs() < 0.05, "unlinked genes stayed together {far}");
    }

    #[test]
    fn dominance_decides_expression() {
        let (g, mut rng) = founder(4, Founder::Grazer);
        let [mut a, mut b] = haplotypes(&g);
        a.toxin = 0.0;
        b.toxin = 1.0;
        let i = 12;
        assert_eq!(loci(&a)[i], 0.0);
        (a.dominance[i], b.dominance[i]) = (128, 128);
        let even = express([a.clone(), b.clone()], g.brain.clone(), g.marker, &mut rng).toxin;
        assert!((even - 0.5).abs() < 1e-4);
        b.dominance[i] = 250;
        let dominant = express([a, b], g.brain.clone(), g.marker, &mut rng).toxin;
        assert!(dominant > 0.95, "{dominant}");
    }

    #[test]
    fn selfing_exposes_hidden_harm() {
        let (mut g, mut rng) = founder(5, Founder::Crawler);
        // A parent carrying many broken genes, each in one copy only.
        let [mut a, b] = haplotypes(&g);
        a.load = 0x00FF_FF00_FFFF_0000;
        g = express([a, b], g.brain.clone(), g.marker, &mut rng);
        assert!(g.vigor > 0.7);
        let n = 300;
        let mut selfed = 0.0;
        for _ in 0..n {
            selfed += g.mate(&g, &mut rng).vigor;
        }
        assert!(selfed / (n as f32) < g.vigor - 0.05, "inbreeding should show the load");
    }

    #[test]
    fn founders_breed_true_but_hybrids_fail() {
        let mut rng = Rng::new(6);
        let a = Genome::founder(Founder::Grazer, &mut rng);
        let b = Genome::founder(Founder::Grazer, &mut rng);
        let (a1, a2) = (a.wild(&mut rng), a.wild(&mut rng));
        let n = 300;
        let within: f32 = (0..n).map(|_| viability(&a1.mate(&a2, &mut rng))).sum::<f32>() / n as f32;
        let across: f32 = (0..n).map(|_| viability(&a1.mate(&b.wild(&mut rng), &mut rng))).sum::<f32>() / n as f32;
        assert!(within > 0.7, "within a founding population {within}");
        assert!(across < 0.2, "between unrelated lineages {across}");
    }

    #[test]
    fn lineages_stay_growable_over_many_generations() {
        let mut rng = Rng::new(7);
        for kind in [Founder::Drifter, Founder::Grazer, Founder::Swimmer, Founder::Crawler] {
            let f = Genome::founder(kind, &mut rng);
            let mut pop: Vec<Genome> = (0..8).map(|_| f.wild(&mut rng)).collect();
            for round in 0..300 {
                let i = rng.below(pop.len());
                let j = rng.below(pop.len());
                let child = if round % 3 == 0 { pop[i].bud(&mut rng) } else { pop[i].mate(&pop[j], &mut rng) };
                let body = crate::body::Body::develop(&child);
                assert!(!body.segments.is_empty());
                assert!(child.parts.len() <= MAX_PARTS);
                pop[rng.below(8)] = child;
            }
        }
    }
}
