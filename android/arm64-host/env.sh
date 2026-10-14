# usage: source env.sh  (in android-build-arm64: arm64 build env, lunch)
cd /Volumes/Android/lineage
export ALLOW_MISSING_DEPENDENCIES=true SKIP_ABI_CHECKS=true THINLTO_USE_MLGO=false
export USE_CCACHE=1 CCACHE_EXEC=/usr/bin/ccache CCACHE_DIR=/Volumes/Android/ccache-arm64
source build/envsetup.sh
lunch lineage_m7-cp2a-userdebug
