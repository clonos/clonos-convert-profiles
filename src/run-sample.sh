#!/bin/sh
if [ ! -r main ]; then
	rustc main.rs && strip main
fi
[ -z "${CIX_DISTDIR}" ] && CIX_DISTDIR="/usr/local/cbsd"
[ -z "${CIX_BIN}" ] && CIX_BIN="/usr/local/bin/cbsd"

CAPABILITIES_LIST_FULL="bhyve qemu xen"
eval $( ${CIX_BIN} capabilities )

CAPABILITIES=
first=1
for i in ${CAPABILITIES_LIST_FULL}; do
	eval _res="\$emulator_${i}_available"
	[ "${_res}" != "1" ] && continue
	if [ ${first} -eq 1 ]; then
		CAPABILITIES="${i}"
	else
		CAPABILITIES="${CAPABILITIES} ${i}"
	fi
	first=0
done

if [ -z "${CAPABILITIES}" ]; then
	echo "no such CAPABILITIES"
	exit 1
fi

PROFILES=$( find ${CIX_DISTDIR}/etc/defaults/ -mindepth 1 -maxdepth 1 -type f -name vm-\*.conf 2>/dev/null | xargs )
if [ -z "${PROFILES}" ]; then
	echo "no such PROFILES"
	exit 1
fi

env \
CIX_PROFILES="${PROFILES}" \
VM_CPUS_MIN=1 \
VM_CPUS_MAX=12 \
VM_RAM_MIN="1g" \
VM_RAM_MAX="16g" \
IMGSIZE_MAX="100g" \
IMGSIZE_MIN="0" \
CIX_PROFILES_DATA="cbsd_vdi_image,cbsd_vdi_user,cbsd_vdi_password,cbsd_vdi_proto,clonos_active,vm_profile,vm_os_type,long_description,default_jailname,imgsize:bytes,imgsize_min:bytes,vm_ram:bytes" \
./main -c "${CAPABILITIES}" -o ./out.php
_ret=$?

exit ${_ret}
