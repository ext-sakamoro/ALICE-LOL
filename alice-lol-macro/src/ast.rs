//! LOL Internal AST (= `Expr` enum + variants)

use proc_macro2::TokenStream as TokenStream2;

/// Value token: either a literal float or a runtime expression.
pub type V = TokenStream2;

#[allow(clippy::enum_variant_names)]
pub enum Expr {
    // ── Primitives (27) ──
    Sphere {
        radius: V,
    },
    Box3d {
        hx: V,
        hy: V,
        hz: V,
    },
    RoundedBox {
        hx: V,
        hy: V,
        hz: V,
        round: V,
    },
    Cylinder {
        radius: V,
        half_height: V,
    },
    Torus {
        major: V,
        minor: V,
    },
    Cone {
        radius: V,
        half_height: V,
    },
    Capsule {
        radius: V,
        half_height: V,
    },
    Ellipsoid {
        rx: V,
        ry: V,
        rz: V,
    },
    Plane {
        nx: V,
        ny: V,
        nz: V,
        d: V,
    },
    Octahedron {
        size: V,
    },
    // v0.4 追加
    RoundedCone {
        r1: V,
        r2: V,
        half_height: V,
    },
    Pyramid {
        half_height: V,
    },
    HexPrism {
        hex_radius: V,
        half_height: V,
    },
    Link {
        half_length: V,
        r1: V,
        r2: V,
    },
    CappedCone {
        half_height: V,
        r1: V,
        r2: V,
    },
    CappedTorus {
        major_radius: V,
        minor_radius: V,
        cap_angle: V,
    },
    RoundedCylinder {
        radius: V,
        round_radius: V,
        half_height: V,
    },
    Tube {
        outer_radius: V,
        thickness: V,
        half_height: V,
    },
    Barrel {
        radius: V,
        half_height: V,
        bulge: V,
    },
    Heart {
        size: V,
    },
    Egg {
        ra: V,
        rb: V,
    },
    Helix {
        major_r: V,
        minor_r: V,
        pitch: V,
        half_height: V,
    },
    Tetrahedron {
        size: V,
    },
    BoxFrame {
        hx: V,
        hy: V,
        hz: V,
        edge: V,
    },
    DiamondPrim {
        radius: V,
        half_height: V,
    },
    StarPolygon {
        radius: V,
        n_points: V,
        m: V,
        half_height: V,
    },
    CrossShape {
        length: V,
        thickness: V,
        round_radius: V,
        half_height: V,
    },

    // ── v1.0 追加プリミティブ (45) ──
    Triangle {
        ax: V,
        ay: V,
        az: V,
        bx: V,
        by: V,
        bz: V,
        cx: V,
        cy: V,
        cz: V,
    },
    BezierPrim {
        ax: V,
        ay: V,
        az: V,
        bx: V,
        by: V,
        bz: V,
        cx: V,
        cy: V,
        cz: V,
        radius: V,
    },
    TriangularPrism {
        width: V,
        half_depth: V,
    },
    CutSphere {
        radius: V,
        cut_height: V,
    },
    CutHollowSphere {
        radius: V,
        cut_height: V,
        thickness: V,
    },
    DeathStar {
        ra: V,
        rb: V,
        d: V,
    },
    SolidAngle {
        angle: V,
        radius: V,
    },
    Rhombus {
        la: V,
        lb: V,
        half_height: V,
        round_radius: V,
    },
    Horseshoe {
        angle: V,
        radius: V,
        half_length: V,
        width: V,
        thickness: V,
    },
    Vesica {
        radius: V,
        half_dist: V,
    },
    InfiniteCylinder {
        radius: V,
    },
    InfiniteCone {
        angle: V,
    },
    GyroidPrim {
        scale: V,
        thickness: V,
    },
    ChamferedCube {
        hx: V,
        hy: V,
        hz: V,
        chamfer: V,
    },
    SchwarzPPrim {
        scale: V,
        thickness: V,
    },
    SuperellipsoidPrim {
        hx: V,
        hy: V,
        hz: V,
        e1: V,
        e2: V,
    },
    RoundedXPrim {
        width: V,
        round_radius: V,
        half_height: V,
    },
    PiePrim {
        angle: V,
        radius: V,
        half_height: V,
    },
    TrapezoidPrim {
        r1: V,
        r2: V,
        trap_height: V,
        half_depth: V,
    },
    ParallelogramPrim {
        width: V,
        para_height: V,
        skew: V,
        half_depth: V,
    },
    TunnelPrim {
        width: V,
        height_2d: V,
        half_depth: V,
    },
    UnevenCapsulePrim {
        r1: V,
        r2: V,
        cap_height: V,
        half_depth: V,
    },
    ArcShapePrim {
        aperture: V,
        radius: V,
        thickness: V,
        half_height: V,
    },
    MoonPrim {
        d: V,
        ra: V,
        rb: V,
        half_height: V,
    },
    BlobbyCrossPrim {
        size: V,
        half_height: V,
    },
    ParabolaSegmentPrim {
        width: V,
        para_height: V,
        half_depth: V,
    },
    RegularPolygonPrim {
        radius: V,
        n_sides: V,
        half_height: V,
    },
    StairsPrim {
        step_width: V,
        step_height: V,
        n_steps: V,
        half_depth: V,
    },
    DodecahedronPrim {
        radius: V,
    },
    IcosahedronPrim {
        radius: V,
    },
    TruncatedOctahedronPrim {
        radius: V,
    },
    TruncatedIcosahedronPrim {
        radius: V,
    },
    DiamondSurfacePrim {
        scale: V,
        thickness: V,
    },
    NeoviusPrim {
        scale: V,
        thickness: V,
    },
    LidinoidPrim {
        scale: V,
        thickness: V,
    },
    IWPPrim {
        scale: V,
        thickness: V,
    },
    FRDPrim {
        scale: V,
        thickness: V,
    },
    FischerKochSPrim {
        scale: V,
        thickness: V,
    },
    PMYPrim {
        scale: V,
        thickness: V,
    },
    Circle2DPrim {
        radius: V,
        half_height: V,
    },
    Rect2DPrim {
        hx: V,
        hy: V,
        half_height: V,
    },
    Segment2DPrim {
        ax: V,
        ay: V,
        bx: V,
        by: V,
        thickness: V,
        half_height: V,
    },
    RoundedRect2DPrim {
        hx: V,
        hy: V,
        round_radius: V,
        half_height: V,
    },
    Annular2DPrim {
        outer_radius: V,
        thickness: V,
        half_height: V,
    },
    // ── v1.0 追加モディファイア ──
    SweepBezierMod {
        p0x: V,
        p0y: V,
        p1x: V,
        p1y: V,
        p2x: V,
        p2y: V,
        child: Box<Self>,
    },
    TerrainPrim {
        scale: V,
        amplitude: V,
    },

    // ── Operations (23) ──
    Union {
        children: Vec<Self>,
    },
    SmoothUnion {
        k: V,
        children: Vec<Self>,
    },
    Intersection {
        children: Vec<Self>,
    },
    SmoothIntersection {
        k: V,
        children: Vec<Self>,
    },
    Subtract {
        a: Box<Self>,
        b: Box<Self>,
    },
    SmoothSubtract {
        k: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    // v0.4 追加
    ChamferUnion {
        r: V,
        children: Vec<Self>,
    },
    ChamferIntersection {
        r: V,
        children: Vec<Self>,
    },
    ChamferSubtraction {
        r: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    StairsUnion {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    StairsIntersection {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    StairsSubtraction {
        r: V,
        n: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Xor {
        a: Box<Self>,
        b: Box<Self>,
    },
    PipeOp {
        r: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Engrave {
        r: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Groove {
        ra: V,
        rb: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    Tongue {
        ra: V,
        rb: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    ColumnsUnion {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    ColumnsIntersection {
        r: V,
        n: V,
        children: Vec<Self>,
    },
    ColumnsSubtraction {
        r: V,
        n: V,
        a: Box<Self>,
        b: Box<Self>,
    },
    ExpSmoothUnion {
        k: V,
        children: Vec<Self>,
    },
    ExpSmoothIntersection {
        k: V,
        children: Vec<Self>,
    },
    ExpSmoothSubtraction {
        k: V,
        a: Box<Self>,
        b: Box<Self>,
    },

    // ── Transforms (4) ──
    Translate {
        x: V,
        y: V,
        z: V,
        child: Box<Self>,
    },
    Rotate {
        rx: V,
        ry: V,
        rz: V,
        child: Box<Self>,
    },
    Scale {
        factor: V,
        child: Box<Self>,
    },
    // v0.4 追加
    ScaleNonUniform {
        sx: V,
        sy: V,
        sz: V,
        child: Box<Self>,
    },

    // ── Time (2) ──
    Animate {
        speed: V,
        amplitude: V,
        child: Box<Self>,
    },
    Morph {
        t: V,
        a: Box<Self>,
        b: Box<Self>,
    },

    // ── Modifiers (19) ──
    Round {
        radius: V,
        child: Box<Self>,
    },
    Onion {
        thickness: V,
        child: Box<Self>,
    },
    Twist {
        strength: V,
        child: Box<Self>,
    },
    Bend {
        curvature: V,
        child: Box<Self>,
    },
    Mirror {
        ax: V,
        ay: V,
        az: V,
        child: Box<Self>,
    },
    Repeat {
        sx: V,
        sy: V,
        sz: V,
        child: Box<Self>,
    },
    // v0.4 追加
    Elongate {
        ax: V,
        ay: V,
        az: V,
        child: Box<Self>,
    },
    Revolution {
        offset: V,
        child: Box<Self>,
    },
    Extrude {
        half_height: V,
        child: Box<Self>,
    },
    Taper {
        factor: V,
        child: Box<Self>,
    },
    Displacement {
        strength: V,
        child: Box<Self>,
    },
    PolarRepeat {
        count: V,
        child: Box<Self>,
    },
    ShearMod {
        xy: V,
        xz: V,
        yz: V,
        child: Box<Self>,
    },
    NoiseMod {
        amplitude: V,
        frequency: V,
        seed: V,
        child: Box<Self>,
    },
    RepeatFinite {
        cx: V,
        cy: V,
        cz: V,
        sx: V,
        sy: V,
        sz: V,
        child: Box<Self>,
    },
    OctantMirror {
        child: Box<Self>,
    },
    IcosahedralSymmetry {
        child: Box<Self>,
    },
    WithMaterial {
        material_id: V,
        child: Box<Self>,
    },
    SurfaceRoughness {
        frequency: V,
        amplitude: V,
        octaves: V,
        child: Box<Self>,
    },

    // ── 3D Print Structural Intent (3) ──
    LatticeInfill {
        shell_thickness: V,
        lattice_scale: V,
        lattice_thickness: V,
        child: Box<Self>,
    },
    DiamondInfill {
        shell_thickness: V,
        lattice_scale: V,
        lattice_thickness: V,
        child: Box<Self>,
    },
    SchwarzInfill {
        shell_thickness: V,
        lattice_scale: V,
        lattice_thickness: V,
        child: Box<Self>,
    },
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
