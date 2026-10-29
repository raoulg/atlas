//! Shaped arrays: an array type annotated with its dtype and the axes of its shape, jaxtyping style
//! (`Float[Tensor, "batch seq dim"]`). The shape is a word over axes; a shape admits another when
//! every array of the one is an array of the other, a preorder the `overrides` principle reads. Its
//! rank (how many axes, or at least how many) is a map from shapes to ranks: its fibers are the
//! rank families, and the family of a shape is the most general shape of its rank, written in the
//! notation itself (`Float[Tensor, _ _]`, `Float[Tensor, ... _]`). Exact shapes stay types of their
//! own: a named axis is a variable bound per call, so `batch dim` and `batch ndim` are two types of
//! one family (`docs/reference/mathematics.md`, "Shapes and their rank").

use std::fmt;

/// One axis of a shape.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Axis {
    /// A size the annotation names (`batch`, `#dim`): one value within a checked call.
    Named(String),
    /// A size no name binds (`_`, `_chunks`): it may differ from array to array.
    Anonymous(String),
    /// A size the annotation fixes (`3`).
    Fixed(u64),
    /// A size an expression gives (`{self.hidden_size}`, `dim-1`).
    Value(String),
    /// Any run of axes (`...`, `*batch`): the shape's rank is open above.
    Variadic(String),
}

impl Axis {
    fn read(word: &str) -> Axis {
        let bare = word.trim_start_matches(['#', '?']);
        let ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');
        if word == "..." || (bare.starts_with('*') && ident(bare.trim_start_matches(['*', '#']))) {
            Axis::Variadic(word.into())
        } else if !bare.is_empty() && bare.chars().all(|c| c.is_ascii_digit()) {
            Axis::Fixed(bare.parse().unwrap_or(u64::MAX))
        } else if bare.starts_with('_') && ident(bare) {
            Axis::Anonymous(word.into())
        } else if ident(bare) {
            Axis::Named(word.into())
        } else {
            Axis::Value(word.into())
        }
    }

    /// Whether this axis, of a promised shape, admits the axis `given` of a given one: a fixed
    /// size only itself, every other axis any one.
    fn admits(&self, given: &Axis) -> bool {
        match self {
            Axis::Fixed(n) => given == &Axis::Fixed(*n),
            _ => true,
        }
    }

    fn fixed(&self) -> bool {
        matches!(self, Axis::Fixed(_))
    }
}

impl fmt::Display for Axis {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Axis::Named(w) | Axis::Anonymous(w) | Axis::Value(w) | Axis::Variadic(w) => f.write_str(w),
            Axis::Fixed(n) => write!(f, "{n}"),
        }
    }
}

/// How many axes a shape has: exactly so many, or at least so many where a run of axes is open.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Rank {
    Exact(usize),
    AtLeast(usize),
}

/// A shaped array: dtype, array type and the axes of its shape (`Float`, `Tensor`, `batch dim`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Array {
    pub dtype: String,
    pub array: String,
    pub axes: Vec<Axis>,
}

impl Array {
    /// A shaped array's type without its library, as a reader writes it: `Float[Tensor, batch dim]`.
    pub fn read(name: &str) -> Option<Array> {
        let (dtype, inner) = name.split_once('[')?;
        let (array, dims) = inner.strip_suffix(']')?.split_once(',')?;
        Some(Array { dtype: dtype.into(), array: array.trim().into(), axes: dims.split_whitespace().map(Axis::read).collect() })
    }

    /// A shaped array's id (`ext:jaxtyping.Float[Tensor, batch dim]`) as an array.
    pub fn of_id(id: &str) -> Option<Array> {
        Array::read(crate::knowledge::shaped(id)?)
    }

    pub fn rank(&self) -> Rank {
        let fixed = self.axes.iter().filter(|a| !matches!(a, Axis::Variadic(_))).count();
        if fixed < self.axes.len() {
            Rank::AtLeast(fixed)
        } else {
            Rank::Exact(fixed)
        }
    }

    /// The most general shape of this one's dtype, array type and rank: every axis anonymous, after
    /// one open run where the rank is open (`_ _`, `... _`). Every shape of the rank fits it.
    pub fn family(&self) -> Array {
        let (open, k) = match self.rank() {
            Rank::Exact(k) => (false, k),
            Rank::AtLeast(k) => (true, k),
        };
        let axes = open.then(|| Axis::Variadic("...".into())).into_iter().chain((0..k).map(|_| Axis::Anonymous("_".into()))).collect();
        Array { dtype: self.dtype.clone(), array: self.array.clone(), axes }
    }

    /// Whether every array of this shape is one of `promised`'s: dtype and array type agree, and
    /// the promised axes admit the given ones, a run of them (`...`, `*name`) any run. Names are not
    /// unified, so a shape that repeats a name is read as one that does not.
    pub fn fits(&self, promised: &Array) -> bool {
        if self.dtype != promised.dtype || self.array != promised.array {
            return false;
        }
        let split = |axes: &[Axis]| match axes.iter().position(|a| matches!(a, Axis::Variadic(_))) {
            Some(i) => (axes[..i].to_vec(), Some(axes[i + 1..].to_vec())),
            None => (axes.to_vec(), None),
        };
        let ((gh, gt), (ph, pt)) = (split(&self.axes), split(&promised.axes));
        match (gt, pt) {
            (None, None) => gh.len() == ph.len() && ph.iter().zip(&gh).all(|(p, g)| p.admits(g)),
            (Some(_), None) => false,
            (None, Some(pt)) => {
                gh.len() >= ph.len() + pt.len()
                    && ph.iter().zip(&gh).all(|(p, g)| p.admits(g))
                    && pt.iter().rev().zip(gh.iter().rev()).all(|(p, g)| p.admits(g))
            }
            // both open: the given shape's own axes cover the promised ones at either end, and where
            // its open run may stand instead, the promised axis admits any
            (Some(gt), Some(pt)) => {
                gh.len() + gt.len() >= ph.len() + pt.len()
                    && ph.iter().enumerate().all(|(j, p)| gh.get(j).map_or(!p.fixed(), |g| p.admits(g)))
                    && pt.iter().rev().enumerate().all(|(j, p)| gt.iter().rev().nth(j).map_or(!p.fixed(), |g| p.admits(g)))
            }
        }
    }
}

impl fmt::Display for Array {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let axes: Vec<String> = self.axes.iter().map(Axis::to_string).collect();
        write!(f, "{}[{}, {}]", self.dtype, self.array, axes.join(" "))
    }
}

/// The id of the rank family of the shaped array `id`, in the same library: `ext:jaxtyping.Float[Tensor, batch dim]`
/// is of `ext:jaxtyping.Float[Tensor, _ _]`.
pub fn family_id(id: &str) -> Option<String> {
    let name = crate::knowledge::shaped(id)?;
    Some(format!("{}{}", &id[..id.len() - name.len()], Array::read(name)?.family()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn a(s: &str) -> Array {
        Array::read(s).unwrap()
    }

    #[test]
    fn each_word_of_a_shape_is_one_axis_or_a_run() {
        let axes = a("Float[Tensor, batch _ _n 3 {self.size} dim-1 #b ... *rest]").axes;
        assert_eq!(
            axes,
            [
                Axis::Named("batch".into()),
                Axis::Anonymous("_".into()),
                Axis::Anonymous("_n".into()),
                Axis::Fixed(3),
                Axis::Value("{self.size}".into()),
                Axis::Value("dim-1".into()),
                Axis::Named("#b".into()),
                Axis::Variadic("...".into()),
                Axis::Variadic("*rest".into()),
            ]
        );
    }

    /// Rank is a map on shapes; its fiber is a family: named, anonymous, fixed and computed axes
    /// count alike, and a run opens the rank above what the other axes count.
    #[test]
    fn rank_counts_the_axes_and_opens_at_a_run() {
        assert_eq!(a("Float[Tensor, batch dim]").rank(), a("Float[Tensor, _ {self.hidden_size}]").rank());
        assert_eq!(a("Float[Tensor, batch dim]").rank(), Rank::Exact(2));
        assert_eq!(a("Float[Tensor, ... {self.out}]").rank(), Rank::AtLeast(1));
        assert_eq!(a("Float[Tensor, batch ...]").rank(), a("Float[Tensor, ... dim]").rank());
        assert_eq!(a("Float[Tensor, ]").rank(), Rank::Exact(0));
    }

    /// Dtype and array type belong to a shape's identity: a family never mixes them.
    #[test]
    fn a_family_keeps_dtype_and_array() {
        let (f, i) = (a("Float[Tensor, batch]").family(), a("Int[Tensor, batch]").family());
        assert_ne!(f, i);
        assert_eq!(f.to_string(), "Float[Tensor, _]");
        assert_ne!(a("Float[ndarray, batch]").family(), f);
    }

    fn axis() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("batch".to_string()),
            Just("dim".to_string()),
            Just("_".to_string()),
            Just("{self.k}".to_string()),
            (1u8..4).prop_map(|n| n.to_string()),
        ]
    }

    /// A shape of at most one run (jaxtyping allows no more), over a small alphabet of axes.
    fn shape() -> impl Strategy<Value = Array> {
        (prop::collection::vec(axis(), 0..4), prop::option::of((0usize..4, prop_oneof![Just("..."), Just("*rest")]))).prop_map(
            |(mut words, run)| {
                if let Some((at, w)) = run {
                    words.insert(at.min(words.len()), w.to_string());
                }
                a(&format!("Float[Tensor, {}]", words.join(" ")))
            },
        )
    }

    /// The concrete shapes of a shape up to `n` axes, sizes 1 to 3, a name or a value any size.
    fn arrays(s: &Array, n: usize) -> Vec<Vec<u64>> {
        let one = |x: &Axis| -> Vec<u64> {
            match x {
                Axis::Fixed(k) => vec![*k],
                _ => vec![1, 2, 3],
            }
        };
        let mut out: Vec<Vec<u64>> = vec![vec![]];
        for x in &s.axes {
            out = match x {
                Axis::Variadic(_) => out
                    .into_iter()
                    .flat_map(|p| {
                        (0..=n.saturating_sub(s.axes.len() - 1)).flat_map(move |k| {
                            let mut runs: Vec<Vec<u64>> = vec![vec![]];
                            for _ in 0..k {
                                runs = runs.into_iter().flat_map(|r| (1..=3).map(move |v| [r.clone(), vec![v]].concat())).collect();
                            }
                            let p = p.clone();
                            runs.into_iter().map(move |r| [p.clone(), r].concat())
                        })
                    })
                    .collect(),
                _ => out.into_iter().flat_map(|p| one(x).into_iter().map(move |v| [p.clone(), vec![v]].concat())).collect(),
            };
        }
        out
    }

    proptest! {
        /// `fits` is containment of the arrays each shape holds: computed on the shapes and computed
        /// on the concrete arrays up to five axes, the two agree.
        #[test]
        fn fits_is_containment(g in shape(), p in shape()) {
            let theirs: std::collections::HashSet<Vec<u64>> = arrays(&p, 5).into_iter().collect();
            let contained = arrays(&g, 5).iter().all(|xs| theirs.contains(xs));
            prop_assert_eq!(g.fits(&p), contained, "{} in {}", g, p);
        }

        /// The family is an upper bound of its fiber, and a projection: every shape fits its
        /// family, and the family of a family is itself.
        #[test]
        fn a_shape_fits_its_family_and_the_family_is_its_own(s in shape()) {
            prop_assert!(s.fits(&s.family()), "{} in {}", s, s.family());
            prop_assert_eq!(s.family().family(), s.family());
            prop_assert_eq!(s.family().rank(), s.rank());
        }
    }
}
