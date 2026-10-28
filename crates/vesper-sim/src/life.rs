//! Birth, growth, injury, sickness, ageing and death, and what becomes of
//! bodies afterwards.

use crate::body::Body;
use crate::brain::Brain;
use crate::creature::{Action, Creature, Tally, adult_matter};
use crate::genome::{Genome, Tissue};
use crate::history::{Behavior, CreatureId, DeathCause, Fact, SpeciesId, StrainId};
use crate::mind::{self, Mind, creature_bin};
use crate::physics::{self, Rig};
use crate::strain::{Infection, Strain};
use crate::think::season_quarter;
use crate::world::{
    ALT_MAX, DEPTH, E_MEAT, E_PER_MATTER, Egg, MAX_CREATURES, Remains, Spores, TICK, World, dist3, pair_mut,
};
use std::sync::Arc;
use vesper_terrain::WORLD_W;

/// Seconds of life for a creature of adult scale 0.01 and ageing 1.
pub const LIFESPAN_K: f32 = 1200.0;

pub fn lifespan(g: &Genome) -> f32 {
    LIFESPAN_K * (g.adult_scale / 0.01).powi(2).max(0.15) / g.aging.max(0.1) + g.growth_time
}

/// How much more matter the body can take on right now.
pub fn growth_room(c: &Creature) -> f32 {
    let adult = c.adult_matter();
    let pace = (c.maturity + TICK / c.genome.growth_time.max(1.0) * 20.0).min(1.0);
    adult * pace * (1.0 + c.pheno.frac(Tissue::Store) * 0.5) - c.matter
}

#[allow(clippy::too_many_arguments)]
pub fn new_creature(
    id: CreatureId,
    genome: Arc<Genome>,
    species: SpeciesId,
    pos: [f32; 3],
    matter: f32,
    parent: Option<CreatureId>,
    mind: Mind,
    generation: u32,
    sterile: bool,
    now: f64,
    heading: f32,
) -> Creature {
    let body = Arc::new(Body::develop(&genome));
    let adult = adult_matter(&genome, &body).max(1e-5);
    let maturity = (matter / adult).clamp(0.02, 1.0);
    let brain = Brain::new(&genome.brain);
    let mut c = Creature {
        id,
        species,
        genome,
        body,
        lost: 0,
        pheno: Default::default(),
        abil: Default::default(),
        maturity,
        pos,
        vel: [0.0; 3],
        heading,
        gait: 0.0,
        matter,
        energy: 0.0,
        health: 1.0,
        wear: 0.0,
        age: 0.0,
        pain: 0.0,
        bleeding: 0.0,
        brain,
        mind,
        action: Action::Idle,
        effort: 0.0,
        flash: 0.0,
        symbol: 0,
        lure: None,
        infections: Vec::new(),
        immunity: Vec::new(),
        sterile,
        parent,
        generation,
        born: now,
        tally: Tally::default(),
        rank: 0.0,
        group: None,
        last_meal: None,
        last_attacker: None,
        breed_wait: 0.0,
        seed: None,
        seed_timer: 0.0,
        reward: 0.0,
        intake: 0.0,
        target: None,
        focus: None,
        doom: None,
        rig: Rig::empty(),
        drive: [0.0; 2],
        // Hopeful about both ways at first, so it tries each.
        stride: 0.5,
        back: 0.5,
        toil: 0.0,
        climb: 0.0,
        exposure: Default::default(),
        shelter: 0.0,
        moments: Vec::new(),
    };
    c.refresh();
    c.rig = Rig::new(&c.body, physics::present(&c.body, c.maturity, 0), c.scale(), pos[2]);
    c.rig.face = if heading.cos() < 0.0 { -1.0 } else { 1.0 };
    c.energy = c.abil.energy_cap * 0.6;
    c
}

/// One creature bites or strikes another. Pieces can come off.
pub fn attack(w: &mut World, ai: usize, di: usize, drive: f32) {
    let dt = TICK;
    let now = w.time;
    let sever_roll = w.rng.f32();
    let pick = w.rng.next_u64();
    let (att, def) = pair_mut(&mut w.creatures, ai, di);
    let raw = att.abil.bite.max(att.abil.spines * att.abil.bite * 0.5 + 0.0001)
        * dt
        * drive.min(1.0)
        * 4.0
        * (1.0 + att.rank.max(0.0) * 0.3);
    let guard = if def.action == Action::Guarding { 0.25 } else { 0.0 };
    let dmg = raw * (1.0 - (def.abil.armor + guard).min(0.9));
    att.action = Action::Attacking;
    att.target = Some(def.id);
    // Fighting is hard work.
    att.energy -= att.abil.upkeep * 2.0 * dt;
    // Spines hurt whatever bites them.
    let back = def.abil.spines * raw * 0.5 / att.matter.max(1e-4);
    att.health -= back;
    att.pain += back * 4.0;
    let frac = dmg / def.matter.max(1e-4);
    // Anything with jaws bites back.
    if def.pheno.has(Tissue::Mouth) && def.health > 0.3 {
        let back = def.abil.bite * dt * 2.0 * (1.0 - att.abil.armor) / att.matter.max(1e-4);
        att.health -= back * 1.2;
        att.pain += back * 4.0;
        att.reward -= (back * 4.0).min(0.5);
    }
    def.health -= frac * 1.6;
    def.pain += frac * 6.0;
    def.bleeding += frac * 0.6;
    def.reward -= (frac * 6.0).min(1.0);
    def.last_attacker = Some((att.id, now));
    let kin = Genome::kinship(&att.genome.marker, &def.genome.marker) > 0.6;
    def.mind.feel(att.id, -0.4, false, now);
    att.mind.feel(def.id, -0.05, kin, now);
    // Hunters eat what they tear off.
    let eats = att.pheno.has(Tissue::Mouth) && att.genome.diet > 0.15;
    let eaten = if eats { (dmg * 0.8).min(def.matter * 0.5) } else { 0.0 };
    def.matter -= eaten;
    let toxin = def.genome.toxin;
    let bin = creature_bin(&def.genome.look, def.length());
    let (def_id, def_species, def_pos, def_heading) = (def.id, def.species, def.pos, def.heading);
    // A limb or segment can come away.
    let mut chunk = None;
    if sever_roll < frac * 2.5 && def.body.segments.len() > 1 {
        let n = def.body.segments.len();
        let seg = 1 + (pick as usize % (n - 1));
        let sub = def.body.subtree(seg);
        if def.lost & sub == 0 && def.body.segments[seg].onset <= def.maturity {
            let part: f32 = def
                .body
                .segments
                .iter()
                .enumerate()
                .filter(|(i, _)| sub & (1 << i) != 0)
                .map(|(_, s)| s.area() * s.tissue.density())
                .sum();
            let total: f32 = def.body.segments.iter().map(|s| s.area() * s.tissue.density()).sum();
            let m = (def.matter * part / total.max(1e-6)).min(def.matter * 0.4);
            def.matter -= m;
            def.lost |= sub;
            def.tally.scars += 1;
            def.bleeding += 0.3;
            def.refresh();
            chunk = Some((sub, m, def.body.clone(), def.scale(), def.genome.look, def.rig.piece(&def.body, sub)));
        }
    }
    let killed = def.health <= 0.0 && def.doom.is_none();
    let guarding = def.action == Action::Guarding;
    // Who it was standing over: its closest kin.
    let defending = def
        .mind
        .known
        .iter()
        .filter(|a| a.kin && a.id != att.id)
        .max_by(|a, b| a.bond.total_cmp(&b.bond))
        .map(|a| a.id);
    let young_def = def.maturity < 0.35;
    let att_parent_of_def = def.parent == Some(att.id);
    let (att_id, att_species, att_diet, att_group, def_group) =
        (att.id, att.species, att.genome.diet, att.group, def.group);
    let lure = att.lure;
    if killed {
        att.tally.kills += 1;
        if att.rank <= def.rank + 0.3 {
            att.rank += 0.1;
        }
    } else if def.health < 0.3 {
        att.rank += 0.02;
        def.rank -= 0.02;
    }
    if eaten > 0.0 {
        // Poison in the flesh.
        let hurt = toxin * eaten * 4.0 / att.matter.max(1e-4);
        att.health -= hurt * 0.3;
        att.pain += hurt;
        let eff = 0.1 + 0.9 * att_diet;
        digest_meat(w, ai, eaten, eff, bin, toxin * 1.5);
    }
    if let Some((sub, m, body, scale, look, chunk_rig)) = chunk {
        let id = w.id();
        w.remains.push(Remains {
            id,
            of: def_id,
            killer: Some(att_id),
            species: def_species,
            body,
            missing: !sub,
            scale,
            look,
            pos: def_pos,
            heading: def_heading,
            matter: m,
            initial: m,
            age: 0.0,
            toxin,
            strain: None,
            offered_to: None,
            rig: Some(chunk_rig),
            moments: Vec::new(),
        });
    }
    // Group against group.
    if att_species == def_species
        && let (Some(ga), Some(gd)) = (att_group, def_group)
        && ga != gd
    {
        w.society.note_attack(ga, gd, def_pos[0], killed);
    }
    if !killed {
        return;
    }
    let cause = if att_species == def_species {
        if young_def && !att_parent_of_def {
            w.first(Behavior::Infanticide, att_species, Some(att_id));
            DeathCause::Infanticide { killer: att_id }
        } else if let (Some(ga), Some(gd)) = (att_group, def_group)
            && ga != gd
        {
            w.first(Behavior::War, att_species, Some(att_id));
            DeathCause::Violence { by_group: ga, killer: att_id }
        } else {
            w.first(Behavior::Cannibalism, att_species, Some(att_id));
            DeathCause::Cannibalism { killer: att_id }
        }
    } else if guarding && let Some(kin) = defending {
        w.first(Behavior::Sacrifice, def_species, Some(def_id));
        DeathCause::Sacrifice { defending: kin }
    } else {
        if att_diet > 0.35 {
            w.first(Behavior::Hunting, att_species, Some(att_id));
        }
        if let Some((m, t)) = lure
            && now - t < 8.0
            && (m == mind::FOOD || m == mind::MATE)
        {
            w.first(Behavior::Deception, att_species, Some(att_id));
        }
        DeathCause::Predation { killer: att_id, killer_species: att_species }
    };
    w.creatures[di].doom = Some(cause);
    w.counters.kills += 1;
}

fn digest_meat(w: &mut World, ci: usize, m: f32, eff: f32, bin: usize, harm: f32) {
    crate::act::digest(w, ci, m, E_MEAT, eff, bin, harm);
}

/// Breeding. With a father, each young mixes both genomes; without, it is a
/// mutated copy. Young are born live, budded, laid as eggs, or scattered as spores.
pub fn breed(w: &mut World, mi: usize, fi: Option<usize>) {
    // Crowding: breeding gets harder well before the hard ceiling, so the
    // population settles below it rather than pressing against it.
    let full = (w.creatures.len() + w.eggs.len()) as f32 / MAX_CREATURES as f32;
    if full >= 1.0 || w.rng.chance(full.powi(6)) {
        return;
    }
    let now = w.time;
    let m = &w.creatures[mi];
    let g = m.genome.clone();
    let adult = m.adult_matter();
    let per = (adult * g.invest).max(1e-5);
    let spare = m.matter - adult * 0.6;
    let mut n = (g.clutch.round() as i32).clamp(1, 40);
    n = n.min((spare / per).floor() as i32);
    let energy_each = per * E_PER_MATTER * 0.6;
    n = n.min((m.energy / energy_each.max(1e-6) * 0.8) as i32);
    if n <= 0 {
        return;
    }
    let father = fi.map(|f| (w.creatures[f].genome.clone(), w.creatures[f].id));
    let (mpos, mid, mspecies, mgen, mheading) = (m.pos, m.id, m.species, m.generation, m.heading);
    let mvoice = m.mind.voice;
    let mhome = m.mind.home;
    let mroute = m.mind.route_from;
    let brood = m.abil.brood;
    let live = g.live_birth > 0.5;
    let spores = g.invest < 0.03 && g.clutch > 8.0;
    {
        let m = &mut w.creatures[mi];
        m.matter -= per * n as f32;
        m.energy -= energy_each * n as f32;
        m.breed_wait = 25.0 * (1.6 - brood) * (1.0 + g.invest * n as f32 * 2.0);
        m.tally.offspring += n as u32;
    }
    if let Some(f) = fi {
        let fc = &mut w.creatures[f];
        fc.breed_wait = 15.0;
        fc.tally.offspring += n as u32;
        fc.reward += 0.5;
        w.first(Behavior::SexualReproduction, mspecies, Some(mid));
    }
    w.creatures[mi].reward += 0.5;
    let mut first_child = None;
    for k in 0..n {
        let child = Arc::new(match &father {
            Some((fg, _)) => g.mate(fg, &mut w.rng),
            None => g.bud(&mut w.rng),
        });
        let species = w.species.classify_creature(&child, mspecies, now, &mut w.history);
        w.species.born(species);
        let sterile = w.rng.chance(g.helpers);
        let jitter = [w.rng.normal() * 0.006, w.rng.normal() * 0.006];
        let mut pos = [mpos[0] + jitter[0], (mpos[1] + jitter[1]).clamp(0.005, DEPTH - 0.005), mpos[2]];
        w.counters.births += 1;
        if live {
            let id = CreatureId(w.id());
            let mind = Mind::inherit(&w.creatures[mi].mind, mid, now);
            let c = new_creature(
                id,
                child,
                species,
                pos,
                per,
                Some(mid),
                mind,
                mgen + 1,
                sterile,
                now,
                mheading + k as f32,
            );
            first_child.get_or_insert(id);
            w.creatures.push(c);
        } else {
            if spores {
                pos[0] = (pos[0] + w.rng.normal() * 0.15).clamp(0.02, WORLD_W - 0.02);
                pos[2] = (pos[2] + w.rng.range(0.0, 0.2)).min(ALT_MAX);
            }
            let id = w.id();
            w.eggs.push(Egg {
                id,
                species,
                genome: child,
                mother: Some(mid),
                voice: mvoice,
                home: mhome,
                route_from: mroute,
                generation: mgen + 1,
                pos,
                matter: per,
                hatch_in: 10.0 + 240.0 * g.invest.sqrt(),
                sterile,
                spore: spores,
            });
        }
    }
    // Some mothers give their own body to the young.
    if live
        && g.matriphagy > 0.6
        && let Some(child) = first_child
        && w.rng.chance((g.matriphagy - 0.6) * 2.5)
    {
        w.creatures[mi].doom = Some(DeathCause::Devoured { by_offspring: child });
    }
}

/// Close contact between two creatures, passing on what they carry.
pub fn contact(w: &mut World, a: usize, b: usize, dt: f32) {
    for (from, to) in [(a, b), (b, a)] {
        let n = w.creatures[from].infections.len();
        for k in 0..n {
            let inf = w.creatures[from].infections[k].clone();
            let s = &w.strains[inf.slot as usize];
            let p = s.contagion * inf.load * dt * s.fit(&w.creatures[to].genome.immune);
            if w.rng.chance(p) {
                infect(w, to, inf.slot as usize);
            }
        }
    }
}

pub fn infect_by_eating(w: &mut World, ci: usize, strain: StrainId) {
    if let Some(slot) = w.strain_slot(strain) {
        let s = &w.strains[slot];
        if !s.gone && w.rng.chance(s.ingestion * s.fit(&w.creatures[ci].genome.immune)) {
            infect(w, ci, slot);
        }
    }
}

/// Infects a creature with a strain, which may change on the way in.
pub fn infect(w: &mut World, ci: usize, slot: usize) {
    let c = &w.creatures[ci];
    let id = w.strains[slot].id;
    if c.infections.len() >= 2 || c.infections.iter().any(|i| i.strain == id) || c.immunity.contains(&id.0) {
        return;
    }
    let mut slot = slot;
    if let Some(mut s) = w.strains[slot].drift(&mut w.rng) {
        s.id = StrainId(w.id());
        s.born = w.time;
        s.origin_host = w.creatures[ci].species;
        let fact = Fact::PathogenArose { id: s.id, parent: s.parent, host: s.origin_host, traits: s.traits() };
        w.strains.push(s);
        slot = w.strains.len() - 1;
        w.record(fact);
    }
    let s = &mut w.strains[slot];
    s.hosts += 1;
    s.gone = false;
    let sid = s.id;
    w.creatures[ci].infections.push(Infection { strain: sid, slot: slot as u32, load: 0.05 });
}

/// A new strain from the soil finds a host.
pub fn emerge(w: &mut World) {
    if w.creatures.is_empty() {
        return;
    }
    let ci = w.rng.below(w.creatures.len());
    let (species, key) = (w.creatures[ci].species, w.creatures[ci].genome.immune);
    let id = StrainId(w.id());
    let s = Strain::wild(id, species, key, w.time, &mut w.rng);
    let fact = Fact::PathogenArose { id, parent: None, host: species, traits: s.traits() };
    w.strains.push(s);
    w.record(fact);
    let slot = w.strains.len() - 1;
    infect(w, ci, slot);
}

/// Strains growing in their hosts, being fought off, and spreading.
pub fn disease(w: &mut World, dt: f32) {
    let n = w.creatures.len();
    for ci in 0..n {
        if w.creatures[ci].infections.is_empty() {
            continue;
        }
        let light = {
            let c = &w.creatures[ci];
            w.env.light(c.pos[0], c.pos[2])
        };
        let mut k = 0;
        while k < w.creatures[ci].infections.len() {
            let slot = w.creatures[ci].infections[k].slot as usize;
            let s = w.strains[slot].clone();
            let c = &mut w.creatures[ci];
            let fit = s.fit(&c.genome.immune);
            let inf = &mut c.infections[k];
            inf.load += s.growth * dt * (0.2 + fit) * (1.0 - inf.load);
            inf.load -= 0.006 * dt * (1.1 - fit) * c.health;
            let load = inf.load;
            c.energy -= s.virulence * load * c.matter.powf(0.75) * dt * 0.6;
            c.energy += s.photosynthetic * load * light * c.matter * 0.04 * dt;
            c.energy = c.energy.min(c.abil.energy_cap);
            if s.virulence > 0.0 {
                c.reward -= s.virulence * load * 0.05;
            }
            if load <= 0.001 {
                c.infections.remove(k);
                c.immunity.push(s.id.0);
                if c.immunity.len() > 8 {
                    c.immunity.remove(0);
                }
                w.strains[slot].hosts = w.strains[slot].hosts.saturating_sub(1);
                continue;
            }
            let (sp, id) = (c.species, c.id);
            if s.bursts > 0.5 && load > 0.97 && c.doom.is_none() {
                c.doom = Some(DeathCause::Parasite { strain: s.id });
                let pos = c.pos;
                w.spores.push(Spores { strain: s.id, pos, strength: 1.0 });
            }
            if load > 0.5 {
                if s.is_symbiont() {
                    w.first(Behavior::Symbiosis, sp, Some(id));
                } else if s.visible > 0.5 && s.virulence > 0.05 {
                    w.first(Behavior::Parasitism, sp, Some(id));
                }
            }
            k += 1;
        }
    }
    // Contact between neighbours.
    let mut pairs = Vec::new();
    for ci in 0..n {
        let c = &w.creatures[ci];
        if c.infections.is_empty() {
            continue;
        }
        let r = c.length() + 0.01;
        w.cgrid.near(c.pos, r, |j| {
            if j != ci && dist3(c.pos, w.creatures[j].pos) < r + w.creatures[j].length() * 0.5 {
                pairs.push((ci, j));
            }
        });
    }
    for (a, b) in pairs {
        contact(w, a, b, dt);
    }
    // Spore clouds.
    let mut s = 0;
    while s < w.spores.len() {
        let sp = w.spores[s].clone();
        let wind = w.env.wind_at(sp.pos[0], sp.pos[2]);
        w.spores[s].pos[0] = (sp.pos[0] + wind * dt).clamp(0.0, WORLD_W);
        w.spores[s].strength *= (-dt / 240.0).exp();
        if w.spores[s].strength < 0.05 {
            w.spores.swap_remove(s);
            continue;
        }
        if let Some(slot) = w.strain_slot(sp.strain) {
            let mut hit = Vec::new();
            w.cgrid.near(sp.pos, 0.04, |j| {
                if dist3(sp.pos, w.creatures[j].pos) < 0.04 {
                    hit.push(j);
                }
            });
            for j in hit {
                let p = sp.strength * 0.05 * dt * w.strains[slot].fit(&w.creatures[j].genome.immune);
                if w.rng.chance(p) {
                    infect(w, j, slot);
                }
            }
        }
        s += 1;
    }
}

/// Upkeep, light-feeding, growth, healing, weather damage and ageing.
pub fn metabolism(w: &mut World, dt: f32) {
    let quarter = season_quarter(w);
    let strike = w.env.lightning >= 1.0;
    if strike {
        w.env.lightning = 0.999;
    }
    let lx = w.env.lightning_x;
    let flood = w.env.river.flood;
    let env = w.env.clone();
    let env = &env;
    for ci in 0..w.creatures.len() {
        let tick = w.tick;
        let roll = w.rng.f32();
        let c = &mut w.creatures[ci];
        if c.doom.is_some() {
            continue;
        }
        c.age += dt;
        let light = env.light(c.pos[0], c.pos[2]);
        let temp = env.temperature(c.pos[0], c.pos[2]);
        // Moving is paid for by the muscles, in the physics.
        let glow_cost = c.flash * c.abil.upkeep * 0.3;
        c.energy -= (c.abil.upkeep + glow_cost) * dt;
        c.energy += c.abil.film * light * dt;
        let cap = c.abil.energy_cap;
        c.energy = c.energy.min(cap);
        if c.energy < 0.0 {
            // Starving: the body feeds on itself.
            let burn = -c.energy / (E_PER_MATTER * 0.5);
            c.matter -= burn;
            c.energy = 0.0;
            c.health -= 0.01 * dt;
            c.reward -= 0.05;
            let pos = c.pos;
            let ill = c.infections.first().map(|i| i.strain);
            let body = c.abil.body_matter;
            let dying = c.matter < body * 0.45;
            if dying {
                c.doom = Some(match ill {
                    Some(s) if roll < 0.7 => DeathCause::Disease { strain: s },
                    _ => DeathCause::Starvation,
                });
            }
            w.shed(pos, burn);
            if dying {
                continue;
            }
        }
        let c = &mut w.creatures[ci];
        let e01 = c.energy / cap.max(1e-6);
        c.reward -= (0.4 - e01).max(0.0) * 0.1;
        // Growing up.
        let adult = c.adult_matter();
        let grown = (c.matter / adult.max(1e-6)).min(1.0);
        if grown > c.maturity {
            let old = c.maturity;
            c.maturity = grown;
            if grown - old > 0.002 && (tick + ci as u64) % 10 == 0 {
                c.refresh();
            }
        }
        // Healing costs energy.
        if c.health < 1.0 && e01 > 0.4 {
            let h = (0.004 * dt).min(1.0 - c.health);
            c.health += h;
            c.energy -= h * c.abil.upkeep * 20.0;
        }
        // Bleeding.
        if c.bleeding > 0.0 {
            c.health -= c.bleeding * 0.02 * dt;
            c.bleeding *= (-dt / 25.0).exp();
            if c.bleeding < 0.01 {
                c.bleeding = 0.0;
            }
        }
        c.pain *= (-dt * 0.5).exp();
        c.mind.fade((-dt * 0.4).exp());
        c.breed_wait -= dt;
        // Weather, worn in over time.
        let (cold, hot) = c.bearable();
        let x = c.pos[0];
        let open = 1.0 - c.shelter.clamp(0.0, 1.0);
        let low = c.pos[2] < 0.05;
        let in_water = low && w.ground.in_river(x, c.pos[1] / DEPTH);
        let damp_ground = if low { w.ground.wetness(x, c.pos[1] / DEPTH) } else { 0.0 };
        let rain = env.rain(x) * open;
        let gust = env.gust(x);
        let humid = env.humidity(x).min(1.0);
        let fog = env.fog_at(x);
        let c = &mut w.creatures[ci];
        let ex = &mut c.exposure;
        ex.soaked += (rain * 0.02 + in_water as u8 as f32 * 0.05) * dt;
        ex.soaked -= ex.soaked * (0.002 + 0.006 * temp.max(0.0) + 0.002 * (gust - 1.0) * open) * dt;
        ex.soaked = ex.soaked.clamp(0.0, 1.0);
        // How warm it feels: sun on its back (dark skins take more of it),
        // wind and wet stripping the heat away in the cold.
        let sun = light * 0.25 * open * (1.3 - 0.6 * c.genome.look[1]);
        let cooling = vesper_terrain::smoothstep(0.6, -0.2, temp);
        let felt = temp + sun - ((gust - 1.0) * 0.12 * open + ex.soaked * 0.35) * cooling;
        if felt < cold {
            ex.chill += (cold - felt) * 0.004 * dt;
        } else {
            ex.chill -= ((felt - cold).min(0.6) * 0.01 + 0.0005) * dt;
        }
        if felt > hot {
            ex.heat += (felt - hot) * 0.004 * (1.4 - 0.6 * humid) * dt;
        } else {
            ex.heat -= ((hot - felt).min(0.6) * 0.01 + 0.0005) * dt;
        }
        // Water: lost faster in heat, dry air and hard work; drunk from wet
        // ground and the river, from rain, and even from fog.
        let loss = (0.25 + 0.75 * temp.max(0.0) + ex.heat) * (1.2 - humid) * (1.0 + c.effort * 0.5) / 1500.0;
        let drink = (damp_ground * 0.04 + in_water as u8 as f32 * 0.06 + rain * 0.02 + fog * 0.004) * dt;
        ex.thirst += loss * dt - drink;
        // Ash and dust breathed in, coughed out slowly.
        let air = (env.dust_at(x) + env.ash_at(x) * 1.3) * (1.0 - 0.6 * c.shelter.clamp(0.0, 1.0));
        ex.grit += air * 0.003 * (1.0 + c.effort) * dt - 0.0006 * dt;
        for v in [&mut ex.chill, &mut ex.heat, &mut ex.thirst, &mut ex.grit] {
            *v = v.clamp(0.0, 1.5);
        }
        let ex = *ex;
        // Shivering, panting and labouring for breath all cost energy.
        c.energy -= c.abil.upkeep * (ex.chill.min(1.0) + 0.5 * ex.heat.min(1.0) + 0.4 * ex.grit.min(1.0)) * dt;
        let hail = env.hail_at(x) * open * (1.0 - c.abil.armor);
        let harms = [
            (ex.chill - 0.6).max(0.0),
            (ex.heat - 0.6).max(0.0),
            (ex.thirst - 0.7).max(0.0) * 0.7,
            (ex.grit - 0.6).max(0.0) * 0.5,
        ];
        let harm: f32 = harms.iter().sum();
        let stress = match harms.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)) {
            Some((k, &v)) if v > 0.0 => Some(k),
            _ => None,
        };
        c.health -= harm * 0.012 * dt + hail * 0.004 * dt;
        c.pain += (harm * 0.15 + hail * 0.3) * dt;
        c.reward -= (ex.chill + ex.heat + ex.thirst * 0.5 + ex.grit * 0.3 - 0.3).max(0.0) * 0.03 + hail * 0.1;
        // Light film burns in the full sun of the day edge.
        if light > 0.85 {
            c.health -= c.pheno.frac(Tissue::LightFilm) * (light - 0.85) * 0.05 * dt;
        }
        // Ageing.
        c.wear += dt / lifespan(&c.genome);
        if c.wear > 0.8 {
            c.health -= (c.wear - 0.8) * 0.02 * dt;
        }
        // Remember where food is good this season.
        c.intake *= (-dt / 30.0).exp();
        if c.intake > 0.4 && e01 > 0.7 {
            let h = &mut c.mind.home[quarter];
            if *h < 0.0 {
                *h = c.pos[0];
                if c.mind.route_from.is_none() {
                    c.mind.route_from = Some(c.id);
                }
            } else {
                *h += (c.pos[0] - *h) * 0.02;
            }
        }
        // Natural disasters.
        let grounded = c.pos[2] < 0.03;
        let doom = if env.lava_at(c.pos[0]) && grounded {
            Some(DeathCause::Lava)
        } else if strike && (c.pos[0] - lx).abs() < 0.015 {
            Some(DeathCause::Lightning)
        } else if flood > 0.6
            && grounded
            && c.length() < 0.02
            && w.ground.in_river(c.pos[0], c.pos[1] / DEPTH)
            && roll < 0.002 * dt
        {
            Some(DeathCause::Flood)
        } else if c.wear >= 1.0 {
            Some(DeathCause::Age)
        } else if c.health <= 0.0 {
            Some(if let Some(k) = stress {
                [DeathCause::Cold, DeathCause::Heat, DeathCause::Thirst, DeathCause::Choked][k]
            } else if let Some(s) = c.infections.iter().max_by(|a, b| a.load.total_cmp(&b.load)) {
                DeathCause::Disease { strain: s.strain }
            } else {
                DeathCause::Starvation
            })
        } else {
            None
        };
        if doom.is_some() {
            c.doom = doom;
        }
        // Seeds eaten with fruit come out somewhere else.
        if c.seed.is_some() {
            c.seed_timer -= dt;
            if c.seed_timer <= 0.0 {
                let seed = c.seed.take().unwrap();
                let (x, d) = (c.pos[0], c.pos[1]);
                crate::ecology::sow(w, &seed, x, d, 0.0, 0.0);
            }
        }
    }
}

/// Lays out the dead: facts for the chronicle, remains for the scavengers.
pub fn bury(w: &mut World) {
    let mut i = w.creatures.len();
    while i > 0 {
        i -= 1;
        let Some(cause) = w.creatures[i].doom else { continue };
        let c = w.creatures.swap_remove(i);
        let mut life = c.life();
        life.is_leader = c.group.is_some_and(|g| w.society.leader_of(g) == Some(c.id));
        w.counters.deaths += 1;
        match cause {
            DeathCause::Starvation => w.counters.starved += 1,
            DeathCause::Age => w.counters.aged += 1,
            DeathCause::Disease { .. } | DeathCause::Parasite { .. } => w.counters.disease += 1,
            DeathCause::Heat | DeathCause::Cold => w.counters.exposure += 1,
            _ => {}
        }
        for inf in &c.infections {
            let s = &mut w.strains[inf.slot as usize];
            s.hosts = s.hosts.saturating_sub(1);
        }
        w.record(Fact::Death { who: c.id, species: c.species, cause, life });
        w.species.died(c.species, w.time, &mut w.history);
        if c.matter <= 0.0 {
            continue;
        }
        if cause == DeathCause::Lava || c.matter < 0.0015 {
            w.to_soil(c.pos[0], c.pos[1], c.matter);
            continue;
        }
        let strain = c
            .infections
            .iter()
            .filter(|inf| w.strains[inf.slot as usize].ingestion > 0.2)
            .max_by(|a, b| a.load.total_cmp(&b.load))
            .map(|inf| inf.strain);
        let killer = match cause {
            DeathCause::Predation { killer, .. }
            | DeathCause::Cannibalism { killer }
            | DeathCause::Infanticide { killer }
            | DeathCause::Violence { killer, .. } => Some(killer),
            _ => None,
        };
        let offered = matches!(cause, DeathCause::Devoured { .. }).then_some(c.id);
        let id = w.id();
        w.remains.push(Remains {
            id,
            of: c.id,
            killer,
            species: c.species,
            body: c.body.clone(),
            missing: c.lost,
            scale: c.scale(),
            look: c.genome.look,
            pos: c.pos,
            heading: c.heading,
            matter: c.matter,
            initial: c.matter,
            age: 0.0,
            toxin: c.genome.toxin,
            strain,
            offered_to: offered,
            rig: Some(c.rig),
            moments: Vec::new(),
        });
        if let Some(seed) = c.seed {
            crate::ecology::sow(w, &seed, c.pos[0], c.pos[1], 0.0, 0.0);
        }
    }
}

/// Bodies sink, rot and return to the soil.
pub fn decay(w: &mut World, dt: f32) {
    let mut i = 0;
    while i < w.remains.len() {
        let r = &mut w.remains[i];
        r.age += dt;
        if r.pos[2] > 0.0 && r.rig.is_none() {
            r.pos[2] = (r.pos[2] - 0.008 * dt).max(0.0);
        }
        if r.age > 150.0 {
            r.strain = None;
        }
        let warm = (w.env.temperature(r.pos[0], 0.0) + 1.3).clamp(0.2, 2.6);
        let wet = w.ground.wetness(r.pos[0], r.pos[1] / DEPTH);
        let rot = r.initial / (40.0 + 400.0 * r.initial.sqrt()) * warm * (1.0 + wet) * dt;
        let m = rot.min(r.matter);
        r.matter -= m;
        let (pos, gone, left) = (r.pos, r.matter < r.initial * 0.03 || r.matter < 1e-5, r.matter);
        w.shed(pos, m);
        if gone {
            w.shed(pos, left);
            w.remains.swap_remove(i);
            continue;
        }
        i += 1;
    }
}

/// Eggs incubate, drift, freeze, cook, get eaten, or hatch.
pub fn eggs(w: &mut World, dt: f32) {
    let now = w.time;
    let mut i = 0;
    while i < w.eggs.len() {
        let e = &mut w.eggs[i];
        e.hatch_in -= dt;
        if e.spore {
            e.pos[0] = (e.pos[0] + w.env.wind_at(e.pos[0], e.pos[2]) * dt).clamp(0.02, WORLD_W - 0.02);
        }
        if e.pos[2] > 0.0 {
            e.pos[2] = (e.pos[2] - 0.004 * dt).max(0.0);
        }
        let temp = w.env.temperature(e.pos[0], e.pos[2]);
        let g = &e.genome;
        let cooked = temp > 0.6 + g.heat_tolerance + 0.4 || w.env.lava_at(e.pos[0]) && e.pos[2] < 0.03;
        let frozen = temp < -(0.6 + g.cold_tolerance + 0.4);
        if e.matter < 1e-6 || cooked || frozen {
            let e = w.eggs.swap_remove(i);
            w.to_soil(e.pos[0], e.pos[1], e.matter.max(0.0));
            w.species.died(e.species, now, &mut w.history);
            continue;
        }
        if e.hatch_in <= 0.0 {
            let e = w.eggs.swap_remove(i);
            let id = CreatureId(w.id());
            let mut mind = Mind::new(e.voice);
            mind.home = e.home;
            mind.route_from = e.route_from;
            if let Some(m) = e.mother {
                mind.known.push(mind::Acquaintance { id: m, bond: 0.6, kin: true, seen: now });
            }
            let heading = w.rng.signed() * std::f32::consts::PI;
            let c = new_creature(
                id,
                e.genome,
                e.species,
                e.pos,
                e.matter,
                e.mother,
                mind,
                e.generation,
                e.sterile,
                now,
                heading,
            );
            w.creatures.push(c);
            continue;
        }
        i += 1;
    }
}
