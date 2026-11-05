use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::multigpu::gbm::GbmGlesBackend;
use smithay::backend::renderer::multigpu::{Error as MultiError, MultiRenderer};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::{ImportAll, ImportMem, Renderer, Texture};
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Transform};

pub(crate) type GpuApi = GbmGlesBackend<GlesRenderer, DrmDeviceFd>;
pub(crate) type TtyRenderer<'render> = MultiRenderer<'render, 'render, GpuApi, GpuApi>;

pub(crate) trait MalaysiaRenderer:
    Renderer<TextureId = Self::MalaysiaTexture, Error = Self::MalaysiaError> + ImportAll + ImportMem
{
    type MalaysiaTexture: Texture + Clone + Send + 'static;
    type MalaysiaError: std::error::Error + Send + Sync + 'static;

    fn gles(&mut self) -> &mut GlesRenderer;

    fn gles_frame<'a, 'frame, 'buffer>(
        frame: &'a mut Self::Frame<'frame, 'buffer>,
    ) -> &'a mut GlesFrame<'frame, 'buffer>
    where
        'buffer: 'frame,
        Self: 'frame;

    fn from_gles_error(error: GlesError) -> Self::Error;
}

impl MalaysiaRenderer for GlesRenderer {
    type MalaysiaTexture = <GlesRenderer as smithay::backend::renderer::RendererSuper>::TextureId;
    type MalaysiaError = GlesError;

    fn gles(&mut self) -> &mut GlesRenderer {
        self
    }

    fn gles_frame<'a, 'frame, 'buffer>(
        frame: &'a mut GlesFrame<'frame, 'buffer>,
    ) -> &'a mut GlesFrame<'frame, 'buffer>
    where
        'buffer: 'frame,
    {
        frame
    }

    fn from_gles_error(error: GlesError) -> GlesError {
        error
    }
}

impl<'render> MalaysiaRenderer for TtyRenderer<'render> {
    type MalaysiaTexture =
        <TtyRenderer<'render> as smithay::backend::renderer::RendererSuper>::TextureId;
    type MalaysiaError = <TtyRenderer<'render> as smithay::backend::renderer::RendererSuper>::Error;

    fn gles(&mut self) -> &mut GlesRenderer {
        self.as_mut()
    }

    fn gles_frame<'a, 'frame, 'buffer>(
        frame: &'a mut Self::Frame<'frame, 'buffer>,
    ) -> &'a mut GlesFrame<'frame, 'buffer>
    where
        'buffer: 'frame,
        Self: 'frame,
    {
        frame.as_mut()
    }

    fn from_gles_error(error: GlesError) -> Self::Error {
        MultiError::Render(error)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PrimaryGpu<E>(pub(crate) E);

impl<E: Element> Element for PrimaryGpu<E> {
    fn id(&self) -> &Id {
        self.0.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.0.current_commit()
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.0.location(scale)
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.0.src()
    }

    fn transform(&self) -> Transform {
        self.0.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.0.geometry(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.0.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.0.opaque_regions(scale)
    }

    fn alpha(&self) -> f32 {
        self.0.alpha()
    }

    fn kind(&self) -> Kind {
        self.0.kind()
    }
}

impl<R: MalaysiaRenderer, E: RenderElement<GlesRenderer>> RenderElement<R> for PrimaryGpu<E> {
    fn draw(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&smithay::utils::user_data::UserDataMap>,
    ) -> Result<(), R::Error> {
        self.0
            .draw(
                R::gles_frame(frame),
                src,
                dst,
                damage,
                opaque_regions,
                cache,
            )
            .map_err(R::from_gles_error)
    }

    fn underlying_storage(&self, _renderer: &mut R) -> Option<UnderlyingStorage<'_>> {
        None
    }
}
