<schema xmlns="http://purl.oclc.org/dsdl/schematron" xmlns:xsl="http://www.w3.org/1999/XSL/Transform" queryBinding="xslt2">
  <ns prefix="t" uri="urn:authored:nodes"/>
  <ns prefix="u" uri="urn:authored:helpers"/>
  <xsl:function name="u:identity" as="xs:string"><xsl:param name="arg"/><xsl:sequence select="string($arg)"/></xsl:function>
  <pattern id="concrete"><rule context="/t:root/t:agreement">
    <assert id="CII-SR-282" flag="warning" test="not(t:seller/t:id)">Authored buyer diagnostic.</assert>
    <assert id="Other" flag="fatal" test="true()">Other diagnostic.</assert>
  </rule></pattern>
</schema>
